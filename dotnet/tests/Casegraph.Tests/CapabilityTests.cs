using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class CapabilityTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:"); GraphDb db = null!; GraphStore owner = null!;
    PayloadCipher Cipher() => new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
    GraphStore Store(GraphCapabilities capabilities) => new(db, Cipher(), new("synthetic", "synthetic-auditor", capabilities));
    public async Task InitializeAsync()
    {
        await connection.OpenAsync(); db = new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options); await GraphMigrator.ApplyAsync(db); owner = Store(GraphCapabilities.All);
    }
    public async Task DisposeAsync() { await db.DisposeAsync(); await connection.DisposeAsync(); }
    [Fact]
    public async Task ReadOnlyContextCanReadVersionsPagesAndAuditButCannotMutateOrExport()
    {
        var entity = await owner.CreateAsync("subject", new { Value = "original" }); await owner.ReviseAsync(entity.Id, 1, new { Value = "current" });
        var read = Store(GraphCapabilities.Read);
        Assert.Equal(2, (await read.GetAsync(entity.Id)).Revision); Assert.Equal(1, (await read.GetVersionAsync(new(entity.Id, 1))).Revision);
        Assert.Single(await read.ListAsync("subject")); Assert.Single(await read.ListPageAsync("subject", 0)); Assert.Single((await read.ReadPageAsync("subject")).Items);
        Assert.True(await read.AnyAsync("subject", _ => true)); Assert.Equal(2, (await read.HistoryAsync(entity.Id)).Count); Assert.Equal(2, (await read.AuditAsync(entity.Id)).Count);
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.CreateAsync("subject", new { }));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.CreateWithIdAsync(Guid.NewGuid(), "subject", new { }));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.ReviseAsync(entity.Id, 2, new { }));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.RelateAsync(entity.Id, "related", entity.Id));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.AddArtifactAsync("synthetic"u8.ToArray(), "text/plain"));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.AddObservationAsync(new(entity.Id, "engine", "1", new string('a', 64), System.Text.Json.JsonSerializer.SerializeToElement(new { }))));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.RecordExportAsync(entity.Id));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => read.RecordVersionExportsAsync([new(entity.Id, 1)]));
        Assert.Equal(2, (await owner.AuditAsync(entity.Id)).Count); Assert.Equal(2, (await owner.GetAsync(entity.Id)).Revision);
    }
    [Fact]
    public async Task ExportCapabilityAllowsAuditedExportWithoutGrantingMutation()
    {
        var entity = await owner.CreateAsync("subject", new { }); var exporter = Store(GraphCapabilities.Read | GraphCapabilities.Export);
        await exporter.RecordVersionExportsAsync([new(entity.Id, 1)]);
        Assert.Contains(await owner.AuditAsync(entity.Id), a => a.Action == "exported");
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => exporter.ReviseAsync(entity.Id, 1, new { }));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => Store(GraphCapabilities.Export).RecordExportAsync(entity.Id));
    }
    [Fact]
    public async Task NoCapabilitiesDeniesReadsAndUndefinedFlagsAreRejected()
    {
        var entity = await owner.CreateAsync("subject", new { }); var denied = Store(GraphCapabilities.None);
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => denied.GetAsync(entity.Id));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => denied.HistoryAsync(entity.Id));
        await Assert.ThrowsAsync<GraphAccessDeniedException>(() => denied.AnyAsync("subject", _ => true));
        Assert.Throws<ArgumentException>(() => new GraphContext("synthetic", "actor", (GraphCapabilities)8));
    }
}
