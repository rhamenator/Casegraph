using System.Security.Cryptography;
using System.Text.Json;
namespace Casegraph;

public sealed record ApprovalData(string Type, int SchemaVersion, EntityVersion Subject, string Decision, string Note,
    string SubjectActor, string Actor, DateTimeOffset At, string Fingerprint);

/// Immutable decisions about exact versions. Consumers transact their own effects with the receipt.
public sealed class GraphApprovalService(GraphStore graph, TimeProvider time)
{
    readonly GraphContext context = graph.Context;
    public static ApprovalData Read(EntitySnapshot entity)
    {
        if (entity.Kind != "receipt" || !entity.Data.TryGetProperty("Type", out var type) || type.GetString() != "casegraph.approval") throw new GraphNotFoundException();
        var data = entity.Data.Deserialize<ApprovalData>()!;
        if (data.SchemaVersion != 1) throw new ArgumentException("Unsupported approval schema.");
        return data;
    }
    static string Fingerprint(EntityVersion subject, string decision, string note)
    {
        if (subject is null || subject.Id == Guid.Empty || subject.Revision < 1 || decision is not ("Approved" or "Declined") || string.IsNullOrWhiteSpace(note) || note.Length > 2000 || note.Contains('\0'))
            throw new ArgumentException("Invalid approval decision.");
        return Convert.ToHexStringLower(SHA256.HashData(JsonSerializer.SerializeToUtf8Bytes(new { subject, decision, note })));
    }
    public async Task<ApprovalData?> ReplayAsync(Guid requestId, EntityVersion subject, string decision, string note, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read | GraphCapabilities.Write);
        if (requestId == Guid.Empty) throw new ArgumentException("Invalid approval request ID.");
        var fingerprint = Fingerprint(subject, decision, note); EntitySnapshot? entity = null;
        try { entity = await graph.GetAsync(requestId, ct); } catch (GraphNotFoundException) { }
        if (entity is null) return null;
        ApprovalData data;
        try { data = Read(entity); } catch (GraphNotFoundException) { throw new GraphConflictException(); }
        if (data.Fingerprint != fingerprint || data.Actor != context.Actor) throw new GraphConflictException();
        return data;
    }
    public async Task<ApprovalData> RecordAsync(Guid requestId, EntityVersion subject, string decision, string note, CancellationToken ct = default)
    {
        var replay = await ReplayAsync(requestId, subject, decision, note, ct); if (replay is not null) return replay;
        var author = await graph.RevisionActorAsync(subject, ct);
        if (author == context.Actor) throw new GraphAccessDeniedException();
        var data = new ApprovalData("casegraph.approval", 1, subject, decision, note, author, context.Actor, time.GetUtcNow(), Fingerprint(subject, decision, note));
        // Outer-transaction callers handle write races and replay after rollback; no partial side effects are inferred here.
        await graph.CreateWithIdAsync(requestId, "receipt", data, ct: ct);
        return data;
    }
}
