using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class FoundationTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:");
    readonly byte[] key = RandomNumberGenerator.GetBytes(32);
    GraphDb Db() => new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options);
    GraphStore Store(GraphDb db, string tenant = "one") => new(db, new PayloadCipher(new KeyRing("v1", new Dictionary<string, byte[]> { ["v1"] = key })), new GraphContext(tenant, "synthetic-staff"));
    public async Task InitializeAsync() { await connection.OpenAsync(); await using var db = Db(); await db.Database.EnsureCreatedAsync(); }
    public async Task DisposeAsync() => await connection.DisposeAsync();
    [Fact]
    public async Task TenantCannotReadReviseRelateOrSeeHistoryAcrossBoundary()
    {
        await using var db = Db(); var one = Store(db); var two = Store(db, "two");
        var entity = await one.CreateAsync("person", new { Name = "Synthetic Example" });
        Assert.Empty(await two.ListAsync("person"));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => two.GetAsync(entity.Id));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => two.ReviseAsync(entity.Id, 1, new { Name = "tamper" }));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => two.HistoryAsync(entity.Id));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => two.AuditAsync(entity.Id));
        var local = await two.CreateAsync("person", new { Name = "Other" });
        await Assert.ThrowsAsync<GraphNotFoundException>(() => two.RelateAsync(local.Id, "knows", entity.Id));
    }
    [Fact]
    public async Task HistorySurvivesContextRestartAndContainsNoPlaintextAtRest()
    {
        Guid id;
        await using (var db = Db())
        {
            var store = Store(db); var entity = await store.CreateAsync("person", new { Name = "SYNTHETIC_SECRET" }); id = entity.Id;
            await store.ReviseAsync(id, 1, new { Name = "Corrected" });
            var history = await store.HistoryAsync(id); Assert.Equal(2, history.Count);
            Assert.Equal("SYNTHETIC_SECRET", history[0].Data.GetProperty("Name").GetString());
            Assert.Equal(2, (await store.AuditAsync(id)).Count);
        }
        await using (var db = Db()) Assert.Equal(2, (await Store(db).GetAsync(id)).Revision);
        using var command = connection.CreateCommand(); command.CommandText = "SELECT EnvelopeJson FROM Records UNION ALL SELECT EnvelopeJson FROM Revisions";
        await using var reader = await command.ExecuteReaderAsync(); while (await reader.ReadAsync()) Assert.DoesNotContain("SYNTHETIC_SECRET", reader.GetString(0));
    }
    [Fact]
    public async Task StaleRevisionAndConcurrentWritersCannotOverwriteHistory()
    {
        await using var firstDb = Db(); var first = Store(firstDb); var entity = await first.CreateAsync("case", new { State = "new" });
        await using var secondDb = Db(); var second = Store(secondDb);
        // Populate second context's tracked copy before the first writer commits.
        await secondDb.Set<GraphRecord>().SingleAsync();
        await first.ReviseAsync(entity.Id, 1, new { State = "first" });
        await Assert.ThrowsAsync<GraphConflictException>(() => second.ReviseAsync(entity.Id, 1, new { State = "second" }));
        await Assert.ThrowsAsync<GraphConflictException>(() => first.ReviseAsync(entity.Id, 1, new { State = "stale" }));
        Assert.Equal(2, (await first.HistoryAsync(entity.Id)).Count);
        Assert.Equal(2, (await first.AuditAsync(entity.Id)).Count);
    }
    [Fact]
    public async Task ArtifactAndObservationCannotBeRevisedAndProvenanceIsRequired()
    {
        await using var db = Db(); var store = Store(db); var artifact = await store.AddArtifactAsync("synthetic"u8.ToArray(), "text/plain");
        var observation = await store.AddObservationAsync(new(artifact.Id, "reader", "1", new string('a', 64), JsonSerializer.SerializeToElement(new { Value = "evidence" })));
        await Assert.ThrowsAsync<InvalidOperationException>(() => store.ReviseAsync(artifact.Id, 1, new { Value = "tamper" }));
        await Assert.ThrowsAsync<InvalidOperationException>(() => store.ReviseAsync(observation.Id, 1, new { Value = "tamper" }));
        await Assert.ThrowsAsync<ArgumentException>(() => store.CreateAsync("observation", new { }));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => store.AddObservationAsync(new(Guid.NewGuid(), "reader", "1", new string('a', 64), JsonSerializer.SerializeToElement(new { }))));
        var revision = await db.Set<GraphRevision>().FirstAsync(); revision.EnvelopeJson = "tamper";
        await Assert.ThrowsAsync<InvalidOperationException>(() => db.SaveChangesAsync());
    }
    [Fact]
    public void EncryptionBindsTenantAndRecordAndSupportsRotation()
    {
        var ring = new KeyRing("old", new Dictionary<string, byte[]> { ["old"] = key }); var cipher = new PayloadCipher(ring);
        var encrypted = cipher.Encrypt("private synthetic text"u8, "tenant-one:record-1");
        Assert.ThrowsAny<CryptographicException>(() => cipher.Decrypt(encrypted, "tenant-two:record-1"));
        var rotated = new PayloadCipher(new KeyRing("new", new Dictionary<string, byte[]> { ["old"] = key, ["new"] = RandomNumberGenerator.GetBytes(32) }));
        Assert.Equal("private synthetic text", Encoding.UTF8.GetString(rotated.Decrypt(encrypted, "tenant-one:record-1")));
        var json = JsonNodeForTamper(encrypted);
        Assert.ThrowsAny<CryptographicException>(() => cipher.Decrypt(json, "tenant-one:record-1"));
    }
    [Fact]
    public async Task WorkflowDefinitionIsImmutableAndHumanReviewGateIsExplicit()
    {
        await using var db = Db(); var graph = Store(db); var workflows = new GraphWorkflowService(graph);
        var subject = await graph.CreateAsync("case", new { Title = "Synthetic" });
        var definition = await workflows.DefineAsync(new("Synthetic workflow", 1, "Received", ["Received", "Reviewed", "Interview"],
            [new("Received", "Reviewed", true), new("Reviewed", "Interview", true)]));
        var instance = await workflows.StartAsync(subject.Id, definition.Id);
        await Assert.ThrowsAsync<ArgumentException>(() => workflows.MoveAsync(instance.Id, 1, "Interview", true));
        await Assert.ThrowsAsync<ArgumentException>(() => workflows.MoveAsync(instance.Id, 1, "Reviewed", false));
        var reviewed = await workflows.MoveAsync(instance.Id, 1, "Reviewed", true);
        Assert.Equal("Reviewed", reviewed.Data.GetProperty("State").GetString());
        await Assert.ThrowsAsync<InvalidOperationException>(() => graph.ReviseAsync(definition.Id, 1, new { Tampered = true }));
        Assert.Equal(2, (await graph.HistoryAsync(instance.Id)).Count);
    }
    [Fact]
    public void SchemaRetainsUnknownDataAndClassifiesFieldsWithoutCoercion()
    {
        var schema = new EntitySchema("person", 1, [new("Name", JsonValueKind.String, true, Sensitivity.Restricted)]);
        var data = JsonSerializer.SerializeToElement(new { Name = 123, Unfamiliar = "preserved" });
        Assert.Contains(SchemaValidator.Validate(schema, data), d => d.Code == "FIELD_TYPE");
        Assert.DoesNotContain(SchemaValidator.Validate(schema, data), d => d.Code == "UNKNOWN_FIELD");
        Assert.Equal("preserved", data.GetProperty("Unfamiliar").GetString());
    }
    [Fact]
    public async Task ExportIsAuditableWithoutPuttingContentInAudit()
    {
        await using var db = Db(); var graph = Store(db); var entity = await graph.CreateAsync("person", new { Secret = "synthetic private content" });
        await graph.RecordExportAsync(entity.Id);
        Assert.Contains(await graph.AuditAsync(entity.Id), row => row.Action == "exported");
        Assert.DoesNotContain("synthetic private content", JsonSerializer.Serialize(await graph.AuditAsync(entity.Id)));
        Assert.Single(await graph.HistoryAsync(entity.Id));
    }
    static string JsonNodeForTamper(string encrypted)
    {
        var node = System.Text.Json.Nodes.JsonNode.Parse(encrypted)!; var bytes = Convert.FromBase64String(node["Ciphertext"]!.GetValue<string>()); bytes[0] ^= 1; node["Ciphertext"] = Convert.ToBase64String(bytes); return node.ToJsonString();
    }
    [Fact]
    public void PostgresUsesJsonbAndCompositeTenantKeys()
    {
        using var db = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseNpgsql("Host=localhost;Database=synthetic").Options);
        var sql = db.Database.GenerateCreateScript(); Assert.Contains("jsonb", sql); Assert.Contains("PRIMARY KEY", sql); Assert.Equal(2, db.Model.FindEntityType(typeof(GraphRecord))!.FindPrimaryKey()!.Properties.Count);
    }
    [Fact]
    public async Task OuterTransactionRollsBackEntireGraphMutation()
    {
        await using var db = Db(); var store = Store(db);
        await using (var transaction = await db.Database.BeginTransactionAsync()) { await store.CreateAsync("case", new { State = "new" }); await transaction.RollbackAsync(); }
        db.ChangeTracker.Clear(); Assert.Empty(await store.ListAsync("case")); Assert.Empty(await db.Set<AuditRow>().ToListAsync());
    }
}
