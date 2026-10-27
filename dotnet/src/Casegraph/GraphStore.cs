using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;
namespace Casegraph;

/// Domain-neutral graph access. Context must originate from authenticated server identity.
public sealed class GraphStore(GraphDb db, PayloadCipher cipher, GraphContext context)
{
    static string Aad(string tenant, string kind, Guid id, int revision, Sensitivity sensitivity) => $"casegraph:v1:{tenant}:{kind}:{id:D}:{revision}:{sensitivity}";
    static void ValidateKind(string kind)
    {
        if (kind.Length is < 1 or > 80 || !kind.All(c => char.IsAsciiLetterOrDigit(c) || c is '.' or '-')) throw new ArgumentException("Invalid entity type.");
    }
    public async Task<EntitySnapshot> CreateAsync<T>(string kind, T data, Sensitivity sensitivity = Sensitivity.Restricted, CancellationToken ct = default)
    {
        ValidateKind(kind);
        if (kind is "artifact" or "observation" or "relationship") throw new ArgumentException("Use the evidence creation methods.");
        return await InsertAsync(kind, data, sensitivity, ct);
    }
    async Task<EntitySnapshot> InsertAsync<T>(string kind, T data, Sensitivity sensitivity, CancellationToken ct)
    {
        if (!Enum.IsDefined(sensitivity)) throw new ArgumentException("Invalid sensitivity.");
        var json = JsonSerializer.SerializeToUtf8Bytes(data);
        if (json.Length > 8 * 1024 * 1024) throw new ArgumentException("Payload exceeds limit.");
        var record = new GraphRecord { Tenant = context.Tenant, Id = Guid.NewGuid(), Kind = kind, Revision = 1, Sensitivity = sensitivity };
        record.EnvelopeJson = cipher.Encrypt(json, Aad(record.Tenant, kind, record.Id, 1, record.Sensitivity));
        db.Records.Add(record); AddHistory(record, "created");
        await SaveAsync(ct);
        return Snapshot(record);
    }
    public async Task<EntitySnapshot> GetAsync(Guid id, CancellationToken ct = default)
    {
        var record = await db.Records.AsNoTracking().SingleOrDefaultAsync(x => x.Tenant == context.Tenant && x.Id == id, ct) ?? throw new GraphNotFoundException();
        return Snapshot(record);
    }
    public async Task<IReadOnlyList<EntitySnapshot>> ListAsync(string kind, CancellationToken ct = default)
    {
        ValidateKind(kind);
        var records = await db.Records.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.Kind == kind).OrderBy(x => x.Id).Take(501).ToListAsync(ct);
        if (records.Count > 500) throw new InvalidOperationException("Use paged listing for more than 500 records.");
        return records.Select(Snapshot).ToArray();
    }
    public async Task<IReadOnlyList<EntitySnapshot>> ListPageAsync(string kind, int offset, int limit = 100, CancellationToken ct = default)
    {
        ValidateKind(kind);
        if (offset < 0 || limit is < 1 or > 100) throw new ArgumentException("Invalid page.");
        var records = await db.Records.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.Kind == kind).OrderBy(x => x.Id).Skip(offset).Take(limit).ToListAsync(ct);
        return records.Select(Snapshot).ToArray();
    }
    public async Task<EntitySnapshot> ReviseAsync<T>(Guid id, int expectedRevision, T data, CancellationToken ct = default)
    {
        var record = await db.Records.SingleOrDefaultAsync(x => x.Tenant == context.Tenant && x.Id == id, ct) ?? throw new GraphNotFoundException();
        if (record.Kind is "artifact" or "observation" or "relationship" or "workflow.definition") throw new InvalidOperationException("Evidence is immutable.");
        if (record.Revision != expectedRevision) throw new GraphConflictException();
        var json = JsonSerializer.SerializeToUtf8Bytes(data);
        if (json.Length > 8 * 1024 * 1024) throw new ArgumentException("Payload exceeds limit.");
        record.Revision++;
        record.EnvelopeJson = cipher.Encrypt(json, Aad(record.Tenant, record.Kind, record.Id, record.Revision, record.Sensitivity));
        AddHistory(record, "revised"); await SaveAsync(ct); return Snapshot(record);
    }
    public async Task<EntitySnapshot> RelateAsync(Guid from, string relationshipType, Guid to, CancellationToken ct = default)
    {
        ValidateKind(relationshipType); await GetAsync(from, ct); await GetAsync(to, ct);
        return await InsertAsync("relationship", new RelationshipData(from, relationshipType, to), Sensitivity.Restricted, ct);
    }
    public async Task<EntitySnapshot> AddArtifactAsync(byte[] bytes, string mediaType, CancellationToken ct = default)
    {
        if (bytes.Length is 0 or > 2097152 || mediaType.Length > 150) throw new ArgumentException("Invalid artifact.");
        return await InsertAsync("artifact", new ArtifactData(Convert.ToHexStringLower(SHA256.HashData(bytes)), mediaType, Convert.ToBase64String(bytes)), Sensitivity.Restricted, ct);
    }
    public async Task<EntitySnapshot> AddObservationAsync(ObservationData observation, CancellationToken ct = default)
    {
        if ((await GetAsync(observation.ArtifactId, ct)).Kind != "artifact") throw new ArgumentException("Source must be an artifact.");
        if (string.IsNullOrWhiteSpace(observation.Engine) || string.IsNullOrWhiteSpace(observation.EngineVersion) || observation.ConfigurationSha256.Length != 64)
            throw new ArgumentException("Processing profile is required.");
        return await InsertAsync("observation", observation, Sensitivity.Restricted, ct);
    }
    public async Task<IReadOnlyList<EntitySnapshot>> HistoryAsync(Guid id, CancellationToken ct = default)
    {
        var current = await GetAsync(id, ct);
        var revisions = await db.Revisions.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.EntityId == id).OrderBy(x => x.Revision).ToListAsync(ct);
        return revisions.Select(x => new EntitySnapshot(id, current.Kind, x.Revision, current.Sensitivity,
            JsonSerializer.Deserialize<JsonElement>(cipher.Decrypt(x.EnvelopeJson, Aad(context.Tenant, current.Kind, id, x.Revision, current.Sensitivity))))).ToArray();
    }
    public async Task<IReadOnlyList<AuditEvent>> AuditAsync(Guid id, CancellationToken ct = default)
    {
        await GetAsync(id, ct);
        return await db.Audit.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.EntityId == id).OrderBy(x => x.Revision)
            .Select(x => new AuditEvent(x.Id, x.Tenant, x.Actor, x.EntityId, x.Revision, x.Action, x.At)).ToListAsync(ct);
    }
    public async Task RecordExportAsync(Guid id, CancellationToken ct = default)
    {
        var record = await GetAsync(id, ct);
        db.Audit.Add(new AuditRow { Id = Guid.NewGuid(), Tenant = context.Tenant, Actor = context.Actor, EntityId = id, Revision = record.Revision, Action = "exported", At = DateTimeOffset.UtcNow });
        await SaveAsync(ct);
    }
    EntitySnapshot Snapshot(GraphRecord record) => new(record.Id, record.Kind, record.Revision, record.Sensitivity,
        JsonSerializer.Deserialize<JsonElement>(cipher.Decrypt(record.EnvelopeJson, Aad(record.Tenant, record.Kind, record.Id, record.Revision, record.Sensitivity))));
    void AddHistory(GraphRecord record, string action)
    {
        db.Revisions.Add(new GraphRevision { Tenant = record.Tenant, EntityId = record.Id, Revision = record.Revision, EnvelopeJson = record.EnvelopeJson });
        db.Audit.Add(new AuditRow { Id = Guid.NewGuid(), Tenant = context.Tenant, Actor = context.Actor, EntityId = record.Id, Revision = record.Revision, Action = action, At = DateTimeOffset.UtcNow });
    }
    async Task SaveAsync(CancellationToken ct)
    {
        try { await db.SaveChangesAsync(ct); }
        catch (DbUpdateConcurrencyException) { db.ChangeTracker.Clear(); throw new GraphConflictException(); }
        // PostgreSQL can insert the competing history row before executing the optimistic UPDATE.
        // Map only this history constraint, preserving unrelated persistence failures.
        catch (DbUpdateException error) when (error.InnerException is Npgsql.PostgresException { SqlState: "23505", ConstraintName: "PK_Revisions" })
        { db.ChangeTracker.Clear(); throw new GraphConflictException(); }
    }
}
