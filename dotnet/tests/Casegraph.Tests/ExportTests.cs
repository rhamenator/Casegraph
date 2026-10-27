using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class ExportTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:"); GraphDb db = null!; GraphStore graph = null!;
    public async Task InitializeAsync()
    {
        await connection.OpenAsync(); db = new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options); await GraphMigrator.ApplyAsync(db);
        graph = new(db, new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] })), new("synthetic", "synthetic-exporter"));
    }
    public async Task DisposeAsync() { await db.DisposeAsync(); await connection.DisposeAsync(); }
    [Fact]
    public async Task ExactHistoricalVersionsAreAuditedWithoutChangingRecordsOrLeakingContent()
    {
        var entity = await graph.CreateAsync("subject", new { Private = "synthetic secret" }); await graph.ReviseAsync(entity.Id, 1, new { Private = "new" });
        await graph.RecordVersionExportsAsync([new(entity.Id, 1), new(entity.Id, 2)]);
        var audit = (await graph.AuditAsync(entity.Id)).Where(a => a.Action == "exported").ToArray();
        Assert.Equal(new[] { 1, 2 }, audit.Select(a => a.Revision)); Assert.All(audit, a => Assert.Equal("synthetic-exporter", a.Actor));
        Assert.DoesNotContain("secret", System.Text.Json.JsonSerializer.Serialize(audit));
        Assert.Equal(2, (await graph.HistoryAsync(entity.Id)).Count);
    }
    [Fact]
    public async Task MissingOrForeignReferenceAndInvalidBoundsLeaveNoPartialExportAudit()
    {
        var entity = await graph.CreateAsync("subject", new { });
        await Assert.ThrowsAsync<GraphNotFoundException>(() => graph.RecordVersionExportsAsync([new(entity.Id, 1), new(Guid.NewGuid(), 1)]));
        var other = new GraphStore(db, new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] })), new("other", "synthetic"));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => other.RecordVersionExportsAsync([new(entity.Id, 1)]));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.RecordVersionExportsAsync([]));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.RecordVersionExportsAsync([new(entity.Id, 1), new(entity.Id, 1)]));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.RecordVersionExportsAsync(Enumerable.Range(1, 101).Select(i => new EntityVersion(entity.Id, i)).ToArray()));
        Assert.DoesNotContain(await graph.AuditAsync(entity.Id), a => a.Action == "exported");
    }
    [Fact]
    public async Task AuditFailureRollsBackWholeBatchAndCallerTransactionsCanAlsoRollBack()
    {
        var one = await graph.CreateAsync("subject", new { }); var two = await graph.CreateAsync("subject", new { });
        await db.Database.ExecuteSqlRawAsync("CREATE TRIGGER synthetic_export_failure BEFORE INSERT ON Audit WHEN NEW.Action='exported' AND (SELECT COUNT(*) FROM Audit WHERE Action='exported')=1 BEGIN SELECT RAISE(ABORT,'synthetic'); END;");
        await Assert.ThrowsAsync<DbUpdateException>(() => graph.RecordVersionExportsAsync([new(one.Id, 1), new(two.Id, 1)]));
        await db.Database.ExecuteSqlRawAsync("DROP TRIGGER synthetic_export_failure;");
        Assert.DoesNotContain(await graph.AuditAsync(one.Id), a => a.Action == "exported"); Assert.DoesNotContain(await graph.AuditAsync(two.Id), a => a.Action == "exported");
        await using (var transaction = await db.Database.BeginTransactionAsync())
        {
            await graph.RecordVersionExportsAsync([new(one.Id, 1), new(two.Id, 1)]); await transaction.RollbackAsync();
        }
        db.ChangeTracker.Clear(); Assert.DoesNotContain(await graph.AuditAsync(one.Id), a => a.Action == "exported");
    }
}
