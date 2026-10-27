using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class VersionTests
{
    [Fact]
    public async Task ExactRevisionReadsSurviveRestartAndDoNotCrossTenantOrFallBackToCurrent()
    {
        await using var connection = new SqliteConnection("Data Source=:memory:"); await connection.OpenAsync();
        var options = new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options;
        var cipher = new PayloadCipher(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
        Guid id;
        await using (var db = new GraphDb(options))
        {
            await GraphMigrator.ApplyAsync(db); var graph = new GraphStore(db, cipher, new("one", "synthetic"));
            id = (await graph.CreateAsync("subject", new { Value = "original" })).Id;
            await graph.ReviseAsync(id, 1, new { Value = "replacement" });
        }
        await using var reopened = new GraphDb(options); var store = new GraphStore(reopened, cipher, new("one", "synthetic"));
        Assert.Equal("original", (await store.GetVersionAsync(new(id, 1))).Data.GetProperty("Value").GetString());
        Assert.Equal("replacement", (await store.GetVersionAsync(new(id, 2))).Data.GetProperty("Value").GetString());
        await Assert.ThrowsAsync<GraphNotFoundException>(() => store.GetVersionAsync(new(id, 3)));
        await Assert.ThrowsAsync<ArgumentException>(() => store.GetVersionAsync(new(id, 0)));
        await Assert.ThrowsAsync<ArgumentException>(() => store.GetVersionAsync(null!));
        await Assert.ThrowsAsync<ArgumentException>(() => store.GetVersionAsync(new(Guid.Empty, 1)));
        var foreign = new GraphStore(reopened, cipher, new("other", "synthetic"));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => foreign.GetVersionAsync(new(id, 1)));
        Assert.Equal(2, (await store.AuditAsync(id)).Count);
    }
}
