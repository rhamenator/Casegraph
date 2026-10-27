using System.Text.Json;
using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
namespace Casegraph.Tests;

public sealed class ManualTime : TimeProvider
{
    DateTimeOffset now = new(2026, 10, 26, 12, 0, 0, TimeSpan.Zero);
    public override DateTimeOffset GetUtcNow() => now;
    public void Advance(TimeSpan duration) => now += duration;
}
public sealed class OperationTests : IAsyncLifetime
{
    readonly SqliteConnection connection = new("Data Source=:memory:");
    readonly ManualTime time = new();
    GraphDb db = null!; GraphStore graph = null!; GraphOperationService operations = null!;
    PayloadCipher Cipher() => new(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
    public async Task InitializeAsync()
    {
        await connection.OpenAsync(); db = new(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options); await GraphMigrator.ApplyAsync(db);
        graph = new(db, Cipher(), new("one", "synthetic-worker")); operations = new(graph, time);
    }
    public async Task DisposeAsync() { await db.DisposeAsync(); await connection.DisposeAsync(); }
    async Task<EntitySnapshot> Queue(int attempts = 3)
    {
        var subject = await graph.CreateAsync("test.subject", new { Label = "synthetic" });
        return await operations.EnqueueAsync(Guid.NewGuid(), "test.processing", subject.Id, 1, JsonSerializer.SerializeToElement(new { Source = "synthetic" }), attempts);
    }
    [Fact]
    public async Task RepeatedRequestsAreIdempotentAndReusedKeysRejectDifferentMeaning()
    {
        var subject = await graph.CreateAsync("test.subject", new { }); var id = Guid.NewGuid();
        var job = await operations.EnqueueAsync(id, "test.processing", subject.Id, 1, JsonSerializer.SerializeToElement(new { A = 1, B = 2 }));
        var replay = await operations.EnqueueAsync(id, "test.processing", subject.Id, 1, JsonSerializer.SerializeToElement(new { B = 2, A = 1 }));
        Assert.Equal(job.Id, replay.Id); Assert.Single(await graph.HistoryAsync(id));
        await Assert.ThrowsAsync<GraphConflictException>(() => operations.EnqueueAsync(id, "test.processing", subject.Id, 1, JsonSerializer.SerializeToElement(new { A = 99 })));
        await Assert.ThrowsAsync<GraphConflictException>(() => operations.EnqueueAsync(subject.Id, "test.processing", subject.Id, 1, JsonSerializer.SerializeToElement(new { })));
        await Assert.ThrowsAsync<GraphConflictException>(() => operations.EnqueueAsync(Guid.NewGuid(), "test.processing", subject.Id, 42, JsonSerializer.SerializeToElement(new { })));
    }
    [Fact]
    public async Task DuplicateStableEntityIdsReturnConflictWithoutReplacingTheOriginal()
    {
        var id = Guid.NewGuid(); await graph.CreateWithIdAsync(id, "test.subject", new { Value = "original synthetic" });
        await Assert.ThrowsAsync<GraphConflictException>(() => graph.CreateWithIdAsync(id, "test.subject", new { Value = "replacement" }));
        await using var otherDb = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options);
        var other = new GraphStore(otherDb, Cipher(), new("one", "synthetic-worker"));
        await Assert.ThrowsAsync<GraphConflictException>(() => other.CreateWithIdAsync(id, "test.subject", new { Value = "replacement" }));
        Assert.Equal("original synthetic", (await graph.GetAsync(id)).Data.GetProperty("Value").GetString());
        Assert.Single(await graph.HistoryAsync(id));
    }
    [Fact]
    public async Task ExpiredLeaseCanBeReclaimedAndFencesTheOldWorker()
    {
        var job = await Queue(); var old = (await operations.ClaimAsync(job.Id, 1, TimeSpan.FromSeconds(10)))!;
        await Assert.ThrowsAsync<OperationNotReadyException>(() => operations.ClaimAsync(job.Id, 2, TimeSpan.FromSeconds(10)));
        time.Advance(TimeSpan.FromSeconds(11));
        var replacement = (await operations.ClaimAsync(job.Id, 2, TimeSpan.FromSeconds(10)))!;
        await Assert.ThrowsAsync<OperationNotReadyException>(() => operations.CompleteAsync(job.Id, old.Token, old.Data.SubjectId));
        var completed = await operations.CompleteAsync(job.Id, replacement.Token, replacement.Data.SubjectId);
        Assert.Equal(OperationStatus.Succeeded, GraphOperationService.Read(completed).Status);
        Assert.Equal(2, GraphOperationService.Read(completed).Attempts);
        Assert.Null(await operations.ClaimAsync(job.Id, completed.Revision, TimeSpan.FromSeconds(10)));
    }
    [Fact]
    public async Task RetryBackoffIsDurableAndAttemptsAreBounded()
    {
        var job = await Queue(2); var first = (await operations.ClaimAsync(job.Id, 1, TimeSpan.FromSeconds(10)))!;
        var retry = await operations.FailAsync(job.Id, first.Token, OperationFailure.TransientProcessing, true);
        Assert.Equal(OperationStatus.RetryPending, GraphOperationService.Read(retry).Status);
        await Assert.ThrowsAsync<OperationNotReadyException>(() => operations.ClaimAsync(job.Id, retry.Revision, TimeSpan.FromSeconds(10)));
        time.Advance(TimeSpan.FromSeconds(5));
        var second = (await operations.ClaimAsync(job.Id, retry.Revision, TimeSpan.FromSeconds(10)))!;
        var failed = await operations.FailAsync(job.Id, second.Token, OperationFailure.TransientProcessing, true);
        Assert.Equal(OperationStatus.Failed, GraphOperationService.Read(failed).Status);
        Assert.Null(await operations.ClaimAsync(job.Id, failed.Revision, TimeSpan.FromSeconds(10)));
        Assert.Equal(2, GraphOperationService.Read(failed).Attempts);
    }
    [Fact]
    public async Task ExhaustedAbandonedLeaseBecomesTerminalWithoutAnotherAttempt()
    {
        var job = await Queue(1); await operations.ClaimAsync(job.Id, 1, TimeSpan.FromSeconds(10)); time.Advance(TimeSpan.FromSeconds(11));
        Assert.Null(await operations.ClaimAsync(job.Id, 2, TimeSpan.FromSeconds(10)));
        var failed = GraphOperationService.Read(await operations.GetAsync(job.Id));
        Assert.Equal(OperationStatus.Failed, failed.Status); Assert.Equal(OperationFailure.AttemptsExhausted, failed.Failure); Assert.Equal(1, failed.Attempts);
    }
    [Fact]
    public async Task SubjectChangeAndTenantBoundaryCannotBeBypassed()
    {
        var job = await Queue();
        var other = new GraphOperationService(new GraphStore(db, Cipher(), new("two", "synthetic-worker")), time);
        await Assert.ThrowsAsync<GraphNotFoundException>(() => other.GetAsync(job.Id));
        await Assert.ThrowsAsync<GraphNotFoundException>(() => other.ClaimAsync(job.Id, 1, TimeSpan.FromSeconds(10)));
        var lease = (await operations.ClaimAsync(job.Id, 1, TimeSpan.FromSeconds(10)))!;
        var superseded = await operations.FailAsync(job.Id, lease.Token, OperationFailure.SubjectChanged, true);
        Assert.Equal(OperationStatus.Superseded, GraphOperationService.Read(superseded).Status);
        Assert.DoesNotContain("Source", JsonSerializer.Serialize(await graph.AuditAsync(job.Id)));
    }
    [Fact]
    public async Task RolledBackCompletionCanBeRecoveredWithoutDuplicateEffects()
    {
        var job = await Queue(); var lease = (await operations.ClaimAsync(job.Id, 1, TimeSpan.FromSeconds(10)))!;
        await using (var transaction = await db.Database.BeginTransactionAsync())
        {
            await graph.CreateAsync("test.effect", new { });
            await operations.CompleteAsync(job.Id, lease.Token, lease.Data.SubjectId);
            await transaction.RollbackAsync();
        }
        db.ChangeTracker.Clear(); Assert.Empty(await graph.ListAsync("test.effect"));
        Assert.Equal(OperationStatus.Running, GraphOperationService.Read(await operations.GetAsync(job.Id)).Status);
        time.Advance(TimeSpan.FromSeconds(11));
        var retry = (await operations.ClaimAsync(job.Id, 2, TimeSpan.FromSeconds(10)))!;
        await using (var transaction = await db.Database.BeginTransactionAsync())
        {
            var effect = await graph.CreateAsync("test.effect", new { }); await operations.CompleteAsync(job.Id, retry.Token, effect.Id); await transaction.CommitAsync();
        }
        Assert.Single(await graph.ListAsync("test.effect"));
    }
    [Fact]
    public async Task OperationSurvivesAReopenedDbContext()
    {
        var job = await Queue(); var lease = (await operations.ClaimAsync(job.Id, 1, TimeSpan.FromSeconds(10)))!;
        await using var reopened = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseSqlite(connection).Options);
        var service = new GraphOperationService(new GraphStore(reopened, Cipher(), new("one", "restarted-worker")), time);
        Assert.Equal(lease.Token, GraphOperationService.Read(await service.GetAsync(job.Id)).LeaseToken);
        time.Advance(TimeSpan.FromSeconds(11));
        Assert.NotNull(await service.ClaimAsync(job.Id, 2, TimeSpan.FromSeconds(10)));
    }
}
