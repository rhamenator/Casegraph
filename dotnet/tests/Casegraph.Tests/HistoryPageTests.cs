using System.Security.Cryptography;
using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class HistoryPageTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:"); GraphDb db = null!; GraphStore graph = null!;
    static PayloadCipher Cipher() => new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
    public async Task InitializeAsync()
    {
        await connection.OpenAsync(); db = new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options); await GraphMigrator.ApplyAsync(db);
        graph = new(db, Cipher(), new("synthetic-history", "author"));
    }
    public async Task DisposeAsync() { await db.DisposeAsync(); await connection.DisposeAsync(); }
    [Fact]
    public async Task SQLitePagesTraverseLargeHistoryAndAuditWithoutTruncation() => await VerifyLargeJournal(db, graph);
    internal static async Task VerifyLargeJournal(GraphDb db, GraphStore graph)
    {
        var entity = await graph.CreateAsync("item", new { Index = 1 });
        await using (var transaction = await db.Database.BeginTransactionAsync())
        {
            for (var revision = 2; revision <= 503; revision++) await graph.ReviseAsync(entity.Id, revision - 1, new { Index = revision });
            // More than one audit event at one revision exercises deterministic tie ordering.
            for (var i = 0; i < 7; i++) await graph.RecordVersionExportsAsync([new(entity.Id, 1)]);
            await transaction.CommitAsync();
        }
        await Assert.ThrowsAsync<InvalidOperationException>(() => graph.HistoryAsync(entity.Id));
        await Assert.ThrowsAsync<InvalidOperationException>(() => graph.AuditAsync(entity.Id));
        var first = await graph.HistoryPageAsync(entity.Id, limit: 100);
        Assert.Equal(503, first.Items[0].Revision); Assert.Equal(404, first.NextBeforeRevision);
        // Concurrent append cannot shift the already-issued revision cursor.
        await graph.ReviseAsync(entity.Id, 503, new { Index = 504 });
        var seen = first.Items.Select(x => x.Revision).ToList(); var before = first.NextBeforeRevision;
        while (before is not null)
        {
            var page = await graph.HistoryPageAsync(entity.Id, before, 100);
            Assert.InRange(page.Items.Count, 1, 100); seen.AddRange(page.Items.Select(x => x.Revision)); before = page.NextBeforeRevision;
        }
        Assert.Equal(Enumerable.Range(1, 503).Reverse(), seen);
        Assert.Empty((await graph.HistoryPageAsync(entity.Id, 1)).Items);
        Assert.Null((await graph.HistoryPageAsync(entity.Id, 101, 100)).NextBeforeRevision);
        var ids = new HashSet<Guid>(); var offset = 0;
        while (true)
        {
            var page = await graph.AuditPageAsync(entity.Id, offset, 100);
            foreach (var item in page.Items) { Assert.True(ids.Add(item.Id)); Assert.Equal(entity.Id, item.EntityId); }
            if (page.NextOffset is not int next) break;
            Assert.Equal(offset + 100, next); offset = next;
        }
        Assert.Equal(511, ids.Count);
        var tied = await graph.AuditPageAsync(entity.Id, 503, 100); var repeated = await graph.AuditPageAsync(entity.Id, 503, 100);
        Assert.Equal(8, tied.Items.Count); Assert.Equal(tied.Items.Select(x => x.Id), repeated.Items.Select(x => x.Id));
        Assert.Null((await graph.AuditPageAsync(entity.Id, 411, 100)).NextOffset);
        Assert.Empty((await graph.AuditPageAsync(entity.Id, 511)).Items);
    }
    [Fact]
    public async Task PagesEnforceCapabilitiesBoundsCancellationAndTenantIsolation()
    {
        var entity = await graph.CreateAsync("item", new { Value = "Synthetic" });
        var reader = new GraphStore(db, Cipher(), new("synthetic-history", "reader", GraphCapabilities.Read));
        Assert.Single((await reader.HistoryPageAsync(entity.Id)).Items); Assert.Single((await reader.AuditPageAsync(entity.Id)).Items);
        var denied = new GraphStore(db, Cipher(), new("synthetic-history", "denied", GraphCapabilities.None));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => denied.HistoryPageAsync(entity.Id));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => denied.AuditPageAsync(entity.Id));
        var other = new GraphStore(db, Cipher(), new("other", "reader"));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => other.HistoryPageAsync(entity.Id));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => other.AuditPageAsync(entity.Id));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => graph.HistoryPageAsync(Guid.NewGuid()));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => graph.AuditPageAsync(Guid.NewGuid()));
        foreach (var limit in new[] { 0, 101 })
        {
            await Assert.ThrowsAsync<ArgumentException>(() => graph.HistoryPageAsync(entity.Id, limit: limit));
            await Assert.ThrowsAsync<ArgumentException>(() => graph.AuditPageAsync(entity.Id, limit: limit));
        }
        await Assert.ThrowsAsync<ArgumentException>(() => graph.HistoryPageAsync(entity.Id, 0));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.AuditPageAsync(entity.Id, -1));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.AuditPageAsync(entity.Id, int.MaxValue));
        using var cancellation = new CancellationTokenSource(); cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => graph.HistoryPageAsync(entity.Id, ct: cancellation.Token));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => graph.AuditPageAsync(entity.Id, ct: cancellation.Token));
    }
    [Fact]
    public async Task LookaheadAndUnselectedRevisionsAreNotDecrypted()
    {
        var entity = await graph.CreateAsync("item", new { Value = "Old key" });
        var keys = new Dictionary<string, byte[]> { ["test"] = new byte[32], ["new"] = Enumerable.Repeat((byte)1, 32).ToArray() };
        var writer = new GraphStore(db, new(new KeyRing("new", keys)), new("synthetic-history", "author"));
        await writer.ReviseAsync(entity.Id, 1, new { Value = "New key" });
        var reader = new GraphStore(db, new(new KeyRing("new", new Dictionary<string, byte[]> { ["new"] = keys["new"] })), new("synthetic-history", "reader", GraphCapabilities.Read));
        var page = await reader.HistoryPageAsync(entity.Id, limit: 1);
        Assert.Equal(2, Assert.Single(page.Items).Revision); Assert.Equal(2, page.NextBeforeRevision);
        Assert.Equal(2, (await reader.GetVersionAsync(new(entity.Id, 2))).Revision);
        Assert.Single((await reader.AuditPageAsync(entity.Id, limit: 1)).Items);
        await Assert.ThrowsAsync<CryptographicException>(() => reader.HistoryPageAsync(entity.Id, 2));
        var oldReader = new GraphStore(db, Cipher(), new("synthetic-history", "reader", GraphCapabilities.Read));
        Assert.Equal(1, (await oldReader.GetVersionAsync(new(entity.Id, 1))).Revision);
        Assert.Equal(1, Assert.Single((await oldReader.HistoryPageAsync(entity.Id, 2)).Items).Revision);
        Assert.Equal(2, (await oldReader.AuditPageAsync(entity.Id)).Items.Count);
    }
}

[Collection("PostgreSQL")]
public sealed class PostgresHistoryPageTests
{
    [PostgresFact]
    public async Task PostgreSQLPagesTraverseLargeHistoryAndAuditWithoutTruncation()
    {
        await using var db = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseNpgsql(Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES")).Options);
        await GraphMigrator.ApplyAsync(db);
        var graph = new GraphStore(db, new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] })), new($"synthetic-history-{Guid.NewGuid():N}", "author"));
        await HistoryPageTests.VerifyLargeJournal(db, graph);
    }
}
