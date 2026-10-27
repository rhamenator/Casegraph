using System.Text.Json;
namespace Casegraph;

public enum OperationStatus { Queued, Running, RetryPending, Succeeded, Failed, Superseded }
public enum OperationFailure { InvalidInput, TransientProcessing, ProcessingFailed, SubjectChanged, AttemptsExhausted }
public sealed record OperationData(string Type, Guid SubjectId, int ExpectedSubjectRevision, JsonElement Parameters,
    OperationStatus Status, int Attempts, int MaxAttempts, Guid? LeaseToken = null, DateTimeOffset? LeaseUntil = null,
    DateTimeOffset? RetryAfter = null, OperationFailure? Failure = null, Guid? ResultId = null, int SchemaVersion = 1);
public sealed record OperationLease(Guid Id, Guid Token, OperationData Data);
public sealed class OperationNotReadyException() : Exception("Operation is not available for processing.");

/// Durable, domain-neutral operation journal. Side effects must commit with CompleteAsync in the same database transaction.
public sealed class GraphOperationService(GraphStore graph, TimeProvider? timeProvider = null)
{
    readonly TimeProvider time = timeProvider ?? TimeProvider.System;
    public static OperationData Read(EntitySnapshot snapshot)
    {
        if (snapshot.Kind != "operation") throw new GraphNotFoundException();
        var data = snapshot.Data.Deserialize<OperationData>() ?? throw new ArgumentException("Invalid operation.");
        if (data.SchemaVersion != 1) throw new ArgumentException("Unsupported operation schema.");
        return data;
    }
    public async Task<EntitySnapshot> GetAsync(Guid id, CancellationToken ct = default)
    {
        var snapshot = await graph.GetAsync(id, ct); Read(snapshot); return snapshot;
    }
    public async Task<EntitySnapshot> EnqueueAsync(Guid requestId, string type, Guid subjectId, int expectedSubjectRevision,
        JsonElement parameters, int maxAttempts = 3, CancellationToken ct = default)
    {
        if (requestId == Guid.Empty || string.IsNullOrWhiteSpace(type) || type.Length > 80 || !type.All(c => char.IsAsciiLetterOrDigit(c) || c is '.' or '-') ||
            expectedSubjectRevision < 1 || maxAttempts is < 1 or > 5 || parameters.ValueKind != JsonValueKind.Object || parameters.GetRawText().Length > 16000)
            throw new ArgumentException("Invalid operation request.");
        EntitySnapshot Match(EntitySnapshot snapshot)
        {
            if (snapshot.Kind != "operation") throw new GraphConflictException();
            var existing = Read(snapshot);
            if (existing.Type != type || existing.SubjectId != subjectId || existing.ExpectedSubjectRevision != expectedSubjectRevision ||
                existing.MaxAttempts != maxAttempts || !JsonElement.DeepEquals(existing.Parameters, parameters)) throw new GraphConflictException();
            return snapshot;
        }
        EntitySnapshot? previous = null;
        try { previous = await graph.GetAsync(requestId, ct); } catch (GraphNotFoundException) { }
        if (previous is not null) return Match(previous);
        var subject = await graph.GetAsync(subjectId, ct);
        if (subject.Revision != expectedSubjectRevision) throw new GraphConflictException();
        var operation = new OperationData(type, subjectId, expectedSubjectRevision, parameters.Clone(), OperationStatus.Queued, 0, maxAttempts);
        try { return await graph.CreateWithIdAsync(requestId, "operation", operation, ct: ct); }
        catch (GraphConflictException) { return Match(await graph.GetAsync(requestId, ct)); }
    }
    public async Task<OperationLease?> ClaimAsync(Guid id, int expectedRevision, TimeSpan leaseDuration, CancellationToken ct = default)
    {
        if (leaseDuration < TimeSpan.FromSeconds(1) || leaseDuration > TimeSpan.FromMinutes(15)) throw new ArgumentException("Invalid lease duration.");
        var snapshot = await GetAsync(id, ct); var data = Read(snapshot); var now = time.GetUtcNow();
        if (snapshot.Revision != expectedRevision) throw new GraphConflictException();
        if (data.Status is OperationStatus.Succeeded or OperationStatus.Failed or OperationStatus.Superseded) return null;
        if (data.Status == OperationStatus.Running && data.LeaseUntil > now || data.Status == OperationStatus.RetryPending && data.RetryAfter > now)
            throw new OperationNotReadyException();
        if (data.Attempts >= data.MaxAttempts)
        {
            await graph.ReviseAsync(id, snapshot.Revision, data with { Status = OperationStatus.Failed, Failure = OperationFailure.AttemptsExhausted, LeaseToken = null, LeaseUntil = null, RetryAfter = null }, ct);
            return null;
        }
        var token = Guid.NewGuid();
        var claimed = data with { Status = OperationStatus.Running, Attempts = data.Attempts + 1, LeaseToken = token, LeaseUntil = now + leaseDuration, RetryAfter = null, Failure = null };
        await graph.ReviseAsync(id, snapshot.Revision, claimed, ct);
        return new(id, token, claimed);
    }
    public async Task<EntitySnapshot> AssertLeaseAsync(Guid id, Guid token, CancellationToken ct = default)
    {
        var snapshot = await GetAsync(id, ct); var data = Read(snapshot);
        if (data.Status != OperationStatus.Running || data.LeaseToken != token || data.LeaseUntil is null || data.LeaseUntil <= time.GetUtcNow())
            throw new OperationNotReadyException();
        return snapshot;
    }
    public async Task<EntitySnapshot> CompleteAsync(Guid id, Guid token, Guid resultId, CancellationToken ct = default)
    {
        await graph.GetAsync(resultId, ct);
        var snapshot = await AssertLeaseAsync(id, token, ct);
        return await graph.ReviseAsync(id, snapshot.Revision, Read(snapshot) with
        { Status = OperationStatus.Succeeded, ResultId = resultId, LeaseToken = null, LeaseUntil = null, RetryAfter = null, Failure = null }, ct);
    }
    public async Task<EntitySnapshot> FailAsync(Guid id, Guid token, OperationFailure failure, bool retryable, CancellationToken ct = default)
    {
        if (!Enum.IsDefined(failure)) throw new ArgumentException("Invalid failure classification.");
        var snapshot = await AssertLeaseAsync(id, token, ct); var data = Read(snapshot);
        var retry = retryable && data.Attempts < data.MaxAttempts && failure != OperationFailure.SubjectChanged;
        var status = failure == OperationFailure.SubjectChanged ? OperationStatus.Superseded : retry ? OperationStatus.RetryPending : OperationStatus.Failed;
        return await graph.ReviseAsync(id, snapshot.Revision, data with
        { Status = status, Failure = failure, LeaseToken = null, LeaseUntil = null, RetryAfter = retry ? time.GetUtcNow() + TimeSpan.FromSeconds(5 * data.Attempts) : null }, ct);
    }
}
