using System.Text;
using System.Text.Json;
using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
namespace Casegraph.Tests;

public static class LegacyFixture
{
    public static PayloadCipher Cipher() => new(new KeyRing("legacy", new Dictionary<string, byte[]> { ["legacy"] = new byte[32] }));
    public static async Task<Guid> SeedAsync(GraphDb db)
    {
        var provider = db.Database.ProviderName == "Microsoft.EntityFrameworkCore.Sqlite" ? "sqlite" : "postgres";
        await db.Database.ExecuteSqlRawAsync(await File.ReadAllTextAsync(Path.Combine(AppContext.BaseDirectory, "Fixtures", $"legacy-v02.{provider}.sql")));
        var id = Guid.NewGuid(); var cipher = Cipher(); var envelope = "";
        for (var revision = 1; revision <= 2; revision++)
        {
            envelope = cipher.Encrypt(Encoding.UTF8.GetBytes(JsonSerializer.Serialize(new { Value = $"synthetic legacy {revision}" })), $"casegraph:v1:legacy:person:{id:D}:{revision}:Restricted");
            db.Set<GraphRevision>().Add(new() { Tenant = "legacy", EntityId = id, Revision = revision, EnvelopeJson = envelope });
            db.Set<AuditRow>().Add(new() { Id = Guid.NewGuid(), Tenant = "legacy", Actor = "synthetic", EntityId = id, Revision = revision, Action = revision == 1 ? "created" : "revised", At = DateTimeOffset.UtcNow });
        }
        db.Set<GraphRecord>().Add(new() { Tenant = "legacy", Id = id, Kind = "person", Revision = 2, Sensitivity = Sensitivity.Restricted, EnvelopeJson = envelope });
        await db.SaveChangesAsync(); db.ChangeTracker.Clear(); return id;
    }
}
public sealed class AuditFailure : SaveChangesInterceptor
{
    public bool Enabled { get; set; }
    public override ValueTask<InterceptionResult<int>> SavingChangesAsync(DbContextEventData data, InterceptionResult<int> result, CancellationToken ct = default)
    {
        if (Enabled && data.Context!.ChangeTracker.Entries<AuditRow>().Any(e => e.State == EntityState.Added))
        { Enabled = false; throw new IOException("synthetic audit interruption"); }
        return ValueTask.FromResult(result);
    }
}
public sealed class MigrationTests
{
    static GraphDb Db(SqliteConnection connection, AuditFailure? failure = null)
    { var options = new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection); if (failure is not null) options.AddInterceptors(failure); return new(options.Options); }
    static GraphStore Store(GraphDb db) => new(db, LegacyFixture.Cipher(), new("legacy", "synthetic"));
    [Fact]
    public async Task InspectionDoesNotInitializeAndFreshMigrationsAreRepeatable()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection);
        var before = await GraphMigrator.StatusAsync(db); Assert.Equal(0, before.CurrentVersion);
        Assert.True(await db.Database.EnsureCreatedAsync()); // Build a legacy empty schema after read-only inspection.
        await Assert.ThrowsAsync<GraphMigrationException>(() => GraphMigrator.ApplyAsync(db));
        Assert.True((await GraphMigrator.StatusAsync(db)).Legacy);
        await GraphMigrator.ApplyAsync(db, true); await GraphMigrator.ApplyAsync(db);
        Assert.Equal(2, (await GraphMigrator.StatusAsync(db)).CurrentVersion);
        Assert.Empty((await GraphMigrator.StatusAsync(db)).Pending);
    }
    [Fact]
    public async Task RejectedPreMigrationWriteDoesNotLingerAfterExplicitAdoption()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection);
        await db.Database.EnsureCreatedAsync(); var graph = Store(db);
        await Assert.ThrowsAsync<GraphMigrationException>(() => graph.CreateAsync("person", new { Value = "rejected synthetic write" }));
        await GraphMigrator.ApplyAsync(db, true); await graph.CreateAsync("person", new { Value = "accepted synthetic write" });
        Assert.Single(await graph.ListAsync("person")); Assert.Single(await db.Set<AuditRow>().ToListAsync());
    }
    [Fact]
    public async Task LegacyAdoptionPreservesEncryptedHistoryAndSubsequentRevisions()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection);
        var id = await LegacyFixture.SeedAsync(db); var encrypted = (await db.Set<GraphRecord>().AsNoTracking().SingleAsync()).EnvelopeJson;
        await Assert.ThrowsAsync<GraphMigrationException>(() => GraphMigrator.ApplyAsync(db));
        Assert.Equal(encrypted, (await db.Set<GraphRecord>().AsNoTracking().SingleAsync()).EnvelopeJson);
        await GraphMigrator.ApplyAsync(db, true);
        var graph = Store(db); Assert.Equal("synthetic legacy 1", (await graph.HistoryAsync(id))[0].Data.GetProperty("Value").GetString());
        Assert.Equal(encrypted, (await db.Set<GraphRecord>().AsNoTracking().SingleAsync()).EnvelopeJson);
        await graph.ReviseAsync(id, 2, new { Value = "synthetic after upgrade" }); Assert.Equal(3, (await graph.HistoryAsync(id)).Count);
    }
    [Fact]
    public async Task InconsistentLegacyHistoryRejectsWithoutPartialMigration()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection);
        await LegacyFixture.SeedAsync(db); await db.Database.ExecuteSqlRawAsync("DELETE FROM \"Revisions\" WHERE \"Revision\"=1");
        await Assert.ThrowsAsync<GraphMigrationException>(() => GraphMigrator.ApplyAsync(db, true));
        Assert.True((await GraphMigrator.StatusAsync(db)).Legacy); Assert.Single(await db.Set<GraphRevision>().ToListAsync());
    }
    [Fact]
    public async Task UnknownSchemaChecksumAndMissingProtectionFailClosed()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection);
        await GraphMigrator.ApplyAsync(db); await db.Database.ExecuteSqlRawAsync("UPDATE \"CasegraphMigrations\" SET \"Sha256\"='unexpected' WHERE \"Version\"=2");
        await Assert.ThrowsAsync<GraphMigrationException>(() => GraphMigrator.StatusAsync(db));
        await db.Database.ExecuteSqlRawAsync("UPDATE \"CasegraphMigrations\" SET \"Sha256\"={0} WHERE \"Version\"=2", GraphMigrator.Scripts("sqlite")[1].Sha256);
        await db.Database.ExecuteSqlRawAsync("DROP TRIGGER cg_records_update");
        await using var fresh = Db(connection); await Assert.ThrowsAsync<GraphMigrationException>(() => GraphMigrator.RequireCurrentAsync(fresh));
    }
    [Theory]
    [InlineData("artifact")]
    [InlineData("observation")]
    [InlineData("relationship")]
    [InlineData("workflow.definition")]
    [InlineData("receipt")]
    public async Task DirectSqlCannotChangeOrDeleteProtectedEvidence(string kind)
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection); await GraphMigrator.ApplyAsync(db);
        // Deliberately write through the database interface, bypassing GraphDb/GraphStore guards.
        var id = Guid.NewGuid().ToString().ToUpperInvariant();
        await db.Database.ExecuteSqlRawAsync("INSERT INTO \"Records\" (\"Tenant\",\"Id\",\"Kind\",\"Revision\",\"Sensitivity\",\"EnvelopeJson\") VALUES ('synthetic',{0},{1},1,3,'{{}}')", id, kind);
        await Assert.ThrowsAsync<SqliteException>(() => db.Database.ExecuteSqlRawAsync("UPDATE \"Records\" SET \"Revision\"=2 WHERE \"Id\"={0}", id));
        await Assert.ThrowsAsync<SqliteException>(() => db.Database.ExecuteSqlRawAsync("DELETE FROM \"Records\" WHERE \"Id\"={0}", id));
        Assert.Single(await db.Set<GraphRevision>().ToListAsync());
    }
    [Fact]
    public async Task HistoryAuditAndMetadataGuardsSurviveBypassingTheOrm()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection); await GraphMigrator.ApplyAsync(db);
        var entity = await Store(db).CreateAsync("person", new { Value = "synthetic" });
        foreach (var sql in new[] { "UPDATE \"Revisions\" SET \"EnvelopeJson\"='{}'", "DELETE FROM \"Revisions\"", "UPDATE \"Audit\" SET \"Actor\"='tampered'", "DELETE FROM \"Audit\"", "UPDATE \"Records\" SET \"Kind\"='other',\"Revision\"=2", "UPDATE \"Records\" SET \"Tenant\"='other',\"Revision\"=2", "UPDATE \"Records\" SET \"Revision\"=99", "DELETE FROM \"Records\"" })
            await Assert.ThrowsAsync<SqliteException>(() => db.Database.ExecuteSqlRawAsync(sql.Replace("{}", "{{}}")));
        Assert.Single(await Store(db).HistoryAsync(entity.Id)); Assert.Single(await Store(db).AuditAsync(entity.Id));
    }
    [Fact]
    public async Task SqliteReplaceCannotBypassGuardsWhenRecursiveTriggersAreDisabled()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection); await GraphMigrator.ApplyAsync(db);
        var graph = Store(db); var artifact = await graph.AddArtifactAsync("synthetic source"u8.ToArray(), "text/plain");
        await db.Database.ExecuteSqlRawAsync("PRAGMA recursive_triggers=OFF");
        foreach (var sql in new[] {
            "INSERT OR REPLACE INTO \"Records\" SELECT * FROM \"Records\"",
            "INSERT OR REPLACE INTO \"Revisions\" SELECT * FROM \"Revisions\"",
            "INSERT OR REPLACE INTO \"Audit\" SELECT * FROM \"Audit\"" })
            await Assert.ThrowsAsync<SqliteException>(() => db.Database.ExecuteSqlRawAsync(sql));
        Assert.Equal("artifact", (await graph.GetAsync(artifact.Id)).Kind); Assert.Single(await graph.HistoryAsync(artifact.Id)); Assert.Single(await graph.AuditAsync(artifact.Id));
    }
    [Fact]
    public async Task FailedProtectionInstallRollsBackAllMigrationDdl()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); await using var db = Db(connection);
        await db.Database.ExecuteSqlRawAsync("CREATE TABLE Unrelated (Value TEXT); INSERT INTO Unrelated VALUES ('synthetic preserved'); CREATE TRIGGER cg_records_insert BEFORE INSERT ON Unrelated BEGIN SELECT 1; END;");
        await Assert.ThrowsAsync<SqliteException>(() => GraphMigrator.ApplyAsync(db));
        Assert.Equal(0, (await GraphMigrator.StatusAsync(db)).CurrentVersion);
        await using var command = connection.CreateCommand(); command.CommandText = "SELECT Value FROM Unrelated"; Assert.Equal("synthetic preserved", await command.ExecuteScalarAsync());
        await db.Database.ExecuteSqlRawAsync("DROP TRIGGER cg_records_insert"); await GraphMigrator.ApplyAsync(db); Assert.Equal(2, (await GraphMigrator.StatusAsync(db)).CurrentVersion);
    }
    [Fact]
    public async Task AuditFailureRollsBackTheTriggerSnapshotAndRecord()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync(); var failure = new AuditFailure(); await using var db = Db(connection, failure); await GraphMigrator.ApplyAsync(db);
        failure.Enabled = true; await Assert.ThrowsAsync<IOException>(() => Store(db).CreateAsync("person", new { Value = "synthetic" }));
        Assert.Empty(await db.Set<GraphRecord>().ToListAsync()); Assert.Empty(await db.Set<GraphRevision>().ToListAsync());
        var entity = await Store(db).CreateAsync("person", new { Value = "recovered" }); Assert.Single(await Store(db).HistoryAsync(entity.Id));
    }
}
