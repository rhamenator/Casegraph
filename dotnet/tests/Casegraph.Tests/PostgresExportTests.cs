using Casegraph;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

[Collection("PostgreSQL")]
public sealed class PostgresExportTests
{
    [PostgresFact]
    public async Task ExactVersionAuditBatchesSurviveRestartAndRespectOuterRollback()
    {
        var options = new DbContextOptionsBuilder<GraphDb>().UseNpgsql(Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES")).Options;
        var cipher = new PayloadCipher(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
        var context = new GraphContext($"synthetic-export-{Guid.NewGuid():N}", "synthetic-exporter"); Guid id;
        await using (var db = new GraphDb(options))
        {
            await GraphMigrator.ApplyAsync(db); var graph = new GraphStore(db, cipher, context);
            id = (await graph.CreateAsync("subject", new { Value = "original" })).Id; await graph.ReviseAsync(id, 1, new { Value = "current" });
            await graph.RecordVersionExportsAsync([new(id, 1), new(id, 2)]);
            await Assert.ThrowsAsync<GraphNotFoundException>(() => graph.RecordVersionExportsAsync([new(id, 1), new(id, 999)]));
            await using (var transaction = await db.Database.BeginTransactionAsync()) { await graph.RecordVersionExportsAsync([new(id, 1)]); await transaction.RollbackAsync(); }
        }
        await using var reopened = new GraphDb(options); var store = new GraphStore(reopened, cipher, context);
        var exports = (await store.AuditAsync(id)).Where(e => e.Action == "exported").ToArray(); Assert.Equal(2, exports.Length);
        Assert.Equal(new[] { 1, 2 }, exports.Select(e => e.Revision)); Assert.All(exports, e => Assert.Equal(context.Actor, e.Actor));
        Assert.Equal("original", (await store.GetVersionAsync(new(id, 1))).Data.GetProperty("Value").GetString());
    }
}
