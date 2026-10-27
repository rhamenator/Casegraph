using System.Security.Cryptography;
using System.Text.Json;
using Casegraph;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class PostgresFactAttribute : FactAttribute
{
    public PostgresFactAttribute() { if (Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES") is null) Skip = "Set CASEGRAPH_TEST_POSTGRES to an isolated synthetic PostgreSQL database."; }
}
public sealed class PostgresTests
{
    [PostgresFact]
    public async Task RealPostgresPersistsJsonbHistoryAndEnforcesTenantAndConcurrency()
    {
        var options = new DbContextOptionsBuilder<GraphDb>().UseNpgsql(Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES")).Options;
        var cipher = new PayloadCipher(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = RandomNumberGenerator.GetBytes(32) }));
        await using var firstDb = new GraphDb(options); await firstDb.Database.EnsureCreatedAsync();
        var tenant = $"synthetic-{Guid.NewGuid():N}";
        var first = new GraphStore(firstDb, cipher, new(tenant, "synthetic-reviewer"));
        var entity = await first.CreateAsync("case", new { State = "new" });
        await using var secondDb = new GraphDb(options); var second = new GraphStore(secondDb, cipher, new(tenant, "synthetic-reviewer"));
        await secondDb.Set<GraphRecord>().SingleAsync(x => x.Tenant == tenant && x.Id == entity.Id);
        await first.ReviseAsync(entity.Id, 1, new { State = "reviewed" });
        await Assert.ThrowsAsync<GraphConflictException>(() => second.ReviseAsync(entity.Id, 1, new { State = "stale" }));
        var artifact = await first.AddArtifactAsync("synthetic original"u8.ToArray(), "text/plain");
        await first.RelateAsync(entity.Id, "has-evidence", artifact.Id);
        await first.AddObservationAsync(new(artifact.Id, "synthetic-engine", "1", new string('a', 64), JsonSerializer.SerializeToElement(new { Text = "synthetic observation" })));
        Assert.Equal(2, (await first.HistoryAsync(entity.Id)).Count);
        var other = new GraphStore(secondDb, cipher, new("other-tenant", "synthetic-reviewer"));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => other.GetAsync(entity.Id));
        await using var command = firstDb.Database.GetDbConnection().CreateCommand();
        await firstDb.Database.OpenConnectionAsync(); command.CommandText = "SELECT jsonb_typeof(\"EnvelopeJson\") FROM \"Records\" LIMIT 1";
        Assert.Equal("object", await command.ExecuteScalarAsync());
    }
}
