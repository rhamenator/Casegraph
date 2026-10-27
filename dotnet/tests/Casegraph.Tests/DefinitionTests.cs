using System.Text.Json;
using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;
public sealed class DefinitionTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:"); GraphDb db = null!;
    GraphStore Store(string actor = "author", string tenant = "one", GraphCapabilities caps = GraphCapabilities.All) => new(db, new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] })), new(tenant, actor, caps));
    GraphDefinitionService Service(string actor = "author", string tenant = "one", GraphCapabilities caps = GraphCapabilities.All) => new(Store(actor, tenant, caps), TimeProvider.System);
    public async Task InitializeAsync() { await connection.OpenAsync(); db = new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options); await GraphMigrator.ApplyAsync(db); }
    public async Task DisposeAsync() { await db.DisposeAsync(); await connection.DisposeAsync(); }
    [Fact]
    public async Task ImmutableVersionsHaveExplicitRootsAndBranchesAndPreserveUnknownContent()
    {
        var root = Guid.NewGuid(); var first = await Service().DefineAsync(root, "generic", "First", JsonSerializer.SerializeToElement(new { Value = 1, Unknown = "preserved" }));
        var child = await Service().DefineAsync(Guid.NewGuid(), "generic", "Revised", JsonSerializer.SerializeToElement(new { Value = 2 }), root);
        var branch = await Service().DefineAsync(Guid.NewGuid(), "generic", "Alternative", JsonSerializer.SerializeToElement(new { Value = 3 }), root);
        Assert.Equal(root, child.RootId); Assert.Equal(2, child.Version); Assert.Equal(2, branch.Version); Assert.Equal(new EntityVersion(root, 1), child.DerivedFrom);
        Assert.Equal("preserved", (await Service().GetAsync(root)).Content.GetProperty("Unknown").GetString());
        await Assert.ThrowsAsync<InvalidOperationException>(() => Store().ReviseAsync(root, 1, new { }));
        Assert.Equal(1, first.Version); Assert.Single(await Store().HistoryAsync(root));
    }
    [Fact]
    public async Task StableReplayMatchesSemanticJsonButRejectsActorBodyAndLineageReuse()
    {
        var id = Guid.NewGuid(); var content = JsonSerializer.Deserialize<JsonElement>("{\"A\":1,\"B\":2}");
        await Service().DefineAsync(id, "generic", "Name", content);
        await Service().DefineAsync(id, "generic", "Name", JsonSerializer.Deserialize<JsonElement>("{\"B\":2,\"A\":1}"));
        await Assert.ThrowsAsync<GraphConflictException>(() => Service("another").DefineAsync(id, "generic", "Name", content));
        await Assert.ThrowsAsync<GraphConflictException>(() => Service().DefineAsync(id, "generic", "Changed", content));
        await Assert.ThrowsAsync<ArgumentException>(() => Service().DefineAsync(Guid.NewGuid(), "other", "Name", content, id));
        Assert.Single(await Store().ListAsync("receipt"));
    }
    [Fact]
    public async Task TenantCapabilitiesAndPayloadBoundsApply()
    {
        var root = Guid.NewGuid(); var content = JsonSerializer.SerializeToElement(new { }); await Service().DefineAsync(root, "generic", "Name", content);
        await Assert.ThrowsAsync<GraphNotFoundException>(() => Service(tenant: "other").GetAsync(root));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => Service(caps: GraphCapabilities.Read).DefineAsync(Guid.NewGuid(), "generic", "Name", content));
        await Assert.ThrowsAsync<ArgumentException>(() => Service().DefineAsync(Guid.NewGuid(), "generic", "Name", JsonSerializer.SerializeToElement(new { Value = new string('x', 65536) })));
        await Assert.ThrowsAsync<ArgumentException>(() => Service().DefineAsync(root, "generic", "Name", content, root));
    }
}
