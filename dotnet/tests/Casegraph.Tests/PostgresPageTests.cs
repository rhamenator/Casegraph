using Casegraph;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

[Collection("PostgreSQL")]
public sealed class PostgresPageTests
{
    [PostgresFact]
    public async Task ProviderPagesAndEncryptedSearchTraversePastFiveHundredWithTenantIsolation()
    {
        var options = new DbContextOptionsBuilder<GraphDb>().UseNpgsql(Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES")).Options;
        await using var db = new GraphDb(options); await GraphMigrator.ApplyAsync(db);
        var cipher = new PayloadCipher(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
        var graph = new GraphStore(db, cipher, new($"synthetic-pages-{Guid.NewGuid():N}", "synthetic"));
        await using (var transaction = await db.Database.BeginTransactionAsync())
        {
            for (var i = 0; i < 503; i++) await graph.CreateWithIdAsync(Guid.Parse($"00000000-0000-0000-0000-{i+1:000000000000}"), "item", new { Index = i });
            await transaction.CommitAsync();
        }
        var ids = new HashSet<Guid>(); var offset = 0;
        while (true)
        {
            var page = await graph.ReadPageAsync("item", offset, 100);
            foreach (var item in page.Items) Assert.True(ids.Add(item.Id));
            if (page.NextOffset is not int next) { Assert.Equal(3, page.Items.Count); break; }
            offset = next;
        }
        Assert.Equal(503, ids.Count);
        var fullLast = await graph.ReadPageAsync("item", 403, 100); Assert.Equal(100, fullLast.Items.Count); Assert.Null(fullLast.NextOffset);
        Assert.True(await graph.AnyAsync("item", item => item.Data.GetProperty("Index").GetInt32() == 502));
        Assert.False(await graph.AnyAsync("item", _ => false));
        var other = new GraphStore(db, cipher, new($"other-{Guid.NewGuid():N}", "synthetic"));
        Assert.Empty((await other.ReadPageAsync("item")).Items); Assert.False(await other.AnyAsync("item", _ => true));
    }
}
