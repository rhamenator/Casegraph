using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class PageTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:");
    GraphDb db = null!; GraphStore graph = null!;
    public async Task InitializeAsync()
    {
        await connection.OpenAsync(); db = new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options); await GraphMigrator.ApplyAsync(db);
        graph = new(db, new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] })), new("one", "synthetic"));
        await using var transaction = await db.Database.BeginTransactionAsync();
        for (var i = 0; i < 503; i++) await graph.CreateWithIdAsync(Guid.Parse($"00000000-0000-0000-0000-{i+1:000000000000}"), "item", new { Index = i });
        await transaction.CommitAsync();
    }
    public async Task DisposeAsync() { await db.DisposeAsync(); await connection.DisposeAsync(); }
    [Fact]
    public async Task PagesTraverseBeyondLegacyBoundWithAccurateContinuationAndNoDuplicateIds()
    {
        await Assert.ThrowsAsync<InvalidOperationException>(() => graph.ListAsync("item"));
        var ids = new HashSet<Guid>(); var offset = 0; var pages = 0;
        while (true)
        {
            var page = await graph.ReadPageAsync("item", offset, 100); Assert.InRange(page.Items.Count, 1, 100); pages++;
            foreach (var item in page.Items) Assert.True(ids.Add(item.Id));
            if (page.NextOffset is not int next) { Assert.Equal(3, page.Items.Count); break; }
            Assert.Equal(offset + 100, next); offset = next;
        }
        Assert.Equal(503, ids.Count); Assert.Equal(6, pages);
        var fullLast = await graph.ReadPageAsync("item", 403, 100); Assert.Equal(100, fullLast.Items.Count); Assert.Null(fullLast.NextOffset);
        Assert.Null((await graph.ReadPageAsync("item", 503)).NextOffset);
        Assert.Empty((await graph.ReadPageAsync("item", 503)).Items);
    }
    [Fact]
    public async Task EncryptedPredicateSearchFindsLateMatchesStopsEarlyAndObservesCancellation()
    {
        var calls = 0;
        Assert.True(await graph.AnyAsync("item", item => { calls++; return item.Data.GetProperty("Index").GetInt32() == 502; })); Assert.Equal(503, calls);
        calls = 0; Assert.True(await graph.AnyAsync("item", _ => { calls++; return true; })); Assert.Equal(1, calls);
        Assert.False(await graph.AnyAsync("item", _ => false));
        using var cancellation = new CancellationTokenSource();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => graph.AnyAsync("item", _ => { cancellation.Cancel(); return false; }, cancellation.Token));
    }
    [Fact]
    public async Task BoundsAndTenantFilteringApplyToPagesAndSearch()
    {
        var other = new GraphStore(db, new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] })), new("other", "synthetic"));
        Assert.Empty((await other.ReadPageAsync("item")).Items); Assert.False(await other.AnyAsync("item", _ => true));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.ReadPageAsync("item", -1));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.ReadPageAsync("item", int.MaxValue));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.ReadPageAsync("item", limit: 101));
        await Assert.ThrowsAsync<ArgumentException>(() => graph.ReadPageAsync("item", limit: 0));
    }
}
