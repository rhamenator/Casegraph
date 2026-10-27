using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class ApprovalTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:"); GraphDb db = null!;
    GraphStore Store(string actor, string tenant = "one", GraphCapabilities caps = GraphCapabilities.All) => new(db, new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] })), new(tenant, actor, caps));
    GraphApprovalService Approvals(string actor, string tenant = "one", GraphCapabilities caps = GraphCapabilities.All) => new(Store(actor, tenant, caps), TimeProvider.System);
    public async Task InitializeAsync() { await connection.OpenAsync(); db = new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options); await GraphMigrator.ApplyAsync(db); }
    public async Task DisposeAsync() { await db.DisposeAsync(); await connection.DisposeAsync(); }
    [Fact]
    public async Task GenericDecisionPinsExactVersionAndRejectsSelfApprovalAndConflictingReplay()
    {
        var store = Store("author"); var entity = await store.CreateAsync("subject", new { Value = "original" });
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => Approvals("author").RecordAsync(Guid.NewGuid(), new(entity.Id, 1), "Approved", "Note"));
        var id = Guid.NewGuid(); var decision = await Approvals("reviewer").RecordAsync(id, new(entity.Id, 1), "Approved", "Reviewed original");
        await store.ReviseAsync(entity.Id, 1, new { Value = "current" });
        Assert.Equal(decision, await Approvals("reviewer").RecordAsync(id, new(entity.Id, 1), "Approved", "Reviewed original"));
        await Assert.ThrowsAsync<GraphConflictException>(() => Approvals("other-reviewer").RecordAsync(id, new(entity.Id, 1), "Approved", "Reviewed original"));
        await Assert.ThrowsAsync<GraphConflictException>(() => Approvals("reviewer").RecordAsync(id, new(entity.Id, 1), "Declined", "Reviewed original"));
        Assert.Equal("author", await store.RevisionActorAsync(new(entity.Id, 1))); Assert.Equal(2, (await store.HistoryAsync(entity.Id)).Count);
    }
    [Fact]
    public async Task ReadOnlyTenantAndInputBoundsFailClosed()
    {
        var entity = await Store("author").CreateAsync("subject", new { });
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => Approvals("reviewer", caps: GraphCapabilities.Read).RecordAsync(Guid.NewGuid(), new(entity.Id, 1), "Approved", "Note"));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => Approvals("reviewer", "other").RecordAsync(Guid.NewGuid(), new(entity.Id, 1), "Approved", "Note"));
        await Assert.ThrowsAsync<ArgumentException>(() => Approvals("reviewer").RecordAsync(Guid.Empty, new(entity.Id, 1), "Approved", "Note"));
        await Assert.ThrowsAsync<ArgumentException>(() => Approvals("reviewer").RecordAsync(Guid.NewGuid(), new(entity.Id, 1), "Unknown", "Note"));
        await Assert.ThrowsAsync<ArgumentException>(() => Approvals("reviewer").RecordAsync(Guid.NewGuid(), new(entity.Id, 1), "Approved", new string('x', 2001)));
        Assert.Empty(await Store("author").ListAsync("receipt"));
    }
    [Fact]
    public async Task AmbiguousMutationAuthorRejectsDecisionRatherThanGuessing()
    {
        var store = Store("author"); var entity = await store.CreateAsync("subject", new { });
        var directId = Guid.NewGuid(); var cipher = new PayloadCipher(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
        db.Set<GraphRecord>().Add(new() { Tenant = "one", Id = directId, Kind = "subject", Revision = 1, Sensitivity = Sensitivity.Restricted,
            EnvelopeJson = cipher.Encrypt("{}"u8, $"casegraph:v1:one:subject:{directId:D}:1:Restricted") });
        await db.SaveChangesAsync();
        await Assert.ThrowsAsync<InvalidOperationException>(() => Approvals("reviewer").RecordAsync(Guid.NewGuid(), new(directId, 1), "Approved", "Missing audit must fail closed"));
        // Privileged audit fabrication must not turn ambiguous attribution into an approval.
        db.Set<AuditRow>().Add(new() { Id = Guid.NewGuid(), Tenant = "one", Actor = "fabricated", EntityId = entity.Id, Revision = 1, Action = "created", At = DateTimeOffset.UtcNow }); await db.SaveChangesAsync();
        await Assert.ThrowsAsync<InvalidOperationException>(() => Approvals("reviewer").RecordAsync(Guid.NewGuid(), new(entity.Id, 1), "Approved", "Note"));
        Assert.Empty(await store.ListAsync("receipt"));
    }
}
