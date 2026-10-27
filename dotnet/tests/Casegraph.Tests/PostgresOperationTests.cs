using System.Text.Json;
using Casegraph;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
namespace Casegraph.Tests;

public sealed class ConcurrentOperationInsert : SaveChangesInterceptor
{
    readonly TaskCompletionSource ready = new(TaskCreationOptions.RunContinuationsAsynchronously);
    int arrivals;
    public override async ValueTask<InterceptionResult<int>> SavingChangesAsync(DbContextEventData data, InterceptionResult<int> result, CancellationToken ct = default)
    {
        if (data.Context!.ChangeTracker.Entries<GraphRecord>().Any(e => e.State == EntityState.Added && e.Entity.Kind == "operation"))
        {
            if (Interlocked.Increment(ref arrivals) == 2) ready.TrySetResult();
            await ready.Task.WaitAsync(TimeSpan.FromSeconds(10), ct);
        }
        return result;
    }
}
[Collection("PostgreSQL")]
public sealed class PostgresOperationTests
{
    [PostgresFact]
    public async Task ConcurrentStableRequestsCreateOneJobAndOnlyOneLeaseWins()
    {
        var connection = Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES");
        var normalOptions = new DbContextOptionsBuilder<GraphDb>().UseNpgsql(connection).Options;
        var barrier = new ConcurrentOperationInsert();
        var raceOptions = new DbContextOptionsBuilder<GraphDb>().UseNpgsql(connection).AddInterceptors(barrier).Options;
        var cipher = new PayloadCipher(new KeyRing("test", new Dictionary<string, byte[]> { ["test"] = new byte[32] }));
        var tenant = $"synthetic-ops-{Guid.NewGuid():N}";
        GraphStore Store(GraphDb db) => new(db, cipher, new(tenant, "synthetic-worker"));
        await using var setup = new GraphDb(normalOptions); await GraphMigrator.ApplyAsync(setup);
        var graph = Store(setup); var subject = await graph.CreateAsync("test.subject", new { Label = "synthetic" });
        var requestId = Guid.NewGuid(); var parameters = JsonSerializer.SerializeToElement(new { Source = "synthetic" });
        await using var oneDb = new GraphDb(raceOptions); await using var twoDb = new GraphDb(raceOptions);
        var one = new GraphOperationService(Store(oneDb)); var two = new GraphOperationService(Store(twoDb));
        var requests = await Task.WhenAll(one.EnqueueAsync(requestId, "test.processing", subject.Id, 1, parameters), two.EnqueueAsync(requestId, "test.processing", subject.Id, 1, parameters));
        Assert.All(requests, result => Assert.Equal(requestId, result.Id));
        Assert.Single(await graph.ListAsync("operation")); Assert.Single(await graph.HistoryAsync(requestId)); Assert.Single(await graph.AuditAsync(requestId));
        async Task<bool> Claim(GraphOperationService service)
        {
            try { return await service.ClaimAsync(requestId, 1, TimeSpan.FromMinutes(2)) is not null; }
            catch (GraphConflictException) { return false; }
        }
        var winners = await Task.WhenAll(Claim(one), Claim(two)); Assert.Equal(1, winners.Count(won => won));
        Assert.Equal(1, GraphOperationService.Read(await graph.GetAsync(requestId)).Attempts);
        Assert.Equal(2, (await graph.HistoryAsync(requestId)).Count);
    }
}
