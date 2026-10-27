using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.EntityFrameworkCore;
namespace Casegraph;

/// Domain-neutral graph access. Context must originate from authenticated server identity.
public sealed class GraphStore(GraphDb db, PayloadCipher cipher, GraphContext context)
{
    internal GraphContext Context => context;
    readonly List<AuditRow> pendingAudits = [];
    static string Aad(string tenant, string kind, Guid id, int revision, Sensitivity sensitivity) => $"casegraph:v1:{tenant}:{kind}:{id:D}:{revision}:{sensitivity}";
    static void ValidateKind(string kind)
    {
        if (kind.Length is < 1 or > 80 || !kind.All(c => char.IsAsciiLetterOrDigit(c) || c is '.' or '-')) throw new ArgumentException("Invalid entity type.");
    }
    public async Task<EntitySnapshot> CreateAsync<T>(string kind, T data, Sensitivity sensitivity = Sensitivity.Restricted, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Write);
        ValidateKind(kind);
        if (kind is "artifact" or "observation" or "relationship") throw new ArgumentException("Use the evidence creation methods.");
        return await InsertAsync(kind, data, sensitivity, ct);
    }
    /// Stable caller-owned IDs support idempotent commands within a tenant.
    public async Task<EntitySnapshot> CreateWithIdAsync<T>(Guid id, string kind, T data, Sensitivity sensitivity = Sensitivity.Restricted, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Write);
        ValidateKind(kind);
        if (id == Guid.Empty || kind is "artifact" or "observation" or "relationship") throw new ArgumentException("Invalid stable entity request.");
        if (await db.Records.AsNoTracking().AnyAsync(x => x.Tenant == context.Tenant && x.Id == id, ct)) throw new GraphConflictException();
        return await InsertAsync(kind, data, sensitivity, ct, id);
    }
    async Task<EntitySnapshot> InsertAsync<T>(string kind, T data, Sensitivity sensitivity, CancellationToken ct, Guid? id = null)
    {
        if (!Enum.IsDefined(sensitivity)) throw new ArgumentException("Invalid sensitivity.");
        var json = JsonSerializer.SerializeToUtf8Bytes(data);
        if (json.Length > 8 * 1024 * 1024) throw new ArgumentException("Payload exceeds limit.");
        var record = new GraphRecord { Tenant = context.Tenant, Id = id ?? Guid.NewGuid(), Kind = kind, Revision = 1, Sensitivity = sensitivity };
        record.EnvelopeJson = cipher.Encrypt(json, Aad(record.Tenant, kind, record.Id, 1, record.Sensitivity));
        db.Records.Add(record); QueueAudit(record, "created");
        await SaveAsync(ct);
        return Snapshot(record);
    }
    public async Task<EntitySnapshot> GetAsync(Guid id, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        var record = await db.Records.AsNoTracking().SingleOrDefaultAsync(x => x.Tenant == context.Tenant && x.Id == id, ct) ?? throw new GraphNotFoundException();
        return Snapshot(record);
    }
    /// Resolve an exact immutable revision without loading the entity's whole history.
    public async Task<EntitySnapshot> GetVersionAsync(EntityVersion version, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        if (version is null || version.Id == Guid.Empty || version.Revision < 1) throw new ArgumentException("Invalid entity version.");
        var current = await MetadataAsync(version.Id, ct);
        var revision = await db.Revisions.AsNoTracking().SingleOrDefaultAsync(x => x.Tenant == context.Tenant && x.EntityId == version.Id && x.Revision == version.Revision, ct) ?? throw new GraphNotFoundException();
        return new(current.Id, current.Kind, revision.Revision, current.Sensitivity,
            JsonSerializer.Deserialize<JsonElement>(cipher.Decrypt(revision.EnvelopeJson, Aad(context.Tenant, current.Kind, current.Id, revision.Revision, current.Sensitivity))));
    }
    public async Task<IReadOnlyList<EntitySnapshot>> ListAsync(string kind, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        ValidateKind(kind);
        var records = await db.Records.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.Kind == kind).OrderBy(x => x.Id).Take(501).ToListAsync(ct);
        if (records.Count > 500) throw new InvalidOperationException("Use paged listing for more than 500 records.");
        return records.Select(Snapshot).ToArray();
    }
    public async Task<IReadOnlyList<EntitySnapshot>> ListPageAsync(string kind, int offset, int limit = 100, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        ValidateKind(kind);
        if (offset < 0 || limit is < 1 or > 100) throw new ArgumentException("Invalid page.");
        var records = await db.Records.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.Kind == kind).OrderBy(x => x.Id).Skip(offset).Take(limit).ToListAsync(ct);
        return records.Select(Snapshot).ToArray();
    }
    /// One bounded page plus lookahead; only returned records are decrypted. Continuation is not a snapshot cursor.
    public async Task<GraphPage<EntitySnapshot>> ReadPageAsync(string kind, int offset = 0, int limit = 50, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        ValidateKind(kind);
        if (offset < 0 || offset > int.MaxValue - 101 || limit is < 1 or > 100) throw new ArgumentException("Invalid page.");
        var records = await db.Records.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.Kind == kind)
            .OrderBy(x => x.Id).Skip(offset).Take(limit + 1).ToListAsync(ct);
        return new(records.Take(limit).Select(Snapshot).ToArray(), records.Count > limit ? checked(offset + limit) : null);
    }
    /// Search encrypted payloads using bounded pages, stopping at the first match. No cleartext index is created.
    public async Task<bool> AnyAsync(string kind, Func<EntitySnapshot, bool> predicate, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        ArgumentNullException.ThrowIfNull(predicate);
        var offset = 0;
        while (true)
        {
            ct.ThrowIfCancellationRequested();
            var page = await ReadPageAsync(kind, offset, 10, ct);
            foreach (var item in page.Items)
            {
                ct.ThrowIfCancellationRequested();
                if (predicate(item)) return true;
            }
            if (page.NextOffset is not int next) return false;
            offset = next;
        }
    }
    public async Task<EntitySnapshot> ReviseAsync<T>(Guid id, int expectedRevision, T data, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Write);
        var record = await db.Records.SingleOrDefaultAsync(x => x.Tenant == context.Tenant && x.Id == id, ct) ?? throw new GraphNotFoundException();
        if (record.Kind is "artifact" or "observation" or "relationship" or "workflow.definition" or "receipt") throw new InvalidOperationException("Evidence is immutable.");
        if (record.Revision != expectedRevision) throw new GraphConflictException();
        var json = JsonSerializer.SerializeToUtf8Bytes(data);
        if (json.Length > 8 * 1024 * 1024) throw new ArgumentException("Payload exceeds limit.");
        record.Revision++;
        record.EnvelopeJson = cipher.Encrypt(json, Aad(record.Tenant, record.Kind, record.Id, record.Revision, record.Sensitivity));
        QueueAudit(record, "revised"); await SaveAsync(ct); return Snapshot(record);
    }
    public async Task<EntitySnapshot> RelateAsync(Guid from, string relationshipType, Guid to, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Write);
        ValidateKind(relationshipType); await GetAsync(from, ct); await GetAsync(to, ct);
        return await InsertAsync("relationship", new RelationshipData(from, relationshipType, to), Sensitivity.Restricted, ct);
    }
    public async Task<EntitySnapshot> AddArtifactAsync(byte[] bytes, string mediaType, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Write);
        if (bytes.Length is 0 or > 2097152 || mediaType.Length > 150) throw new ArgumentException("Invalid artifact.");
        return await InsertAsync("artifact", new ArtifactData(Convert.ToHexStringLower(SHA256.HashData(bytes)), mediaType, Convert.ToBase64String(bytes)), Sensitivity.Restricted, ct);
    }
    public async Task<EntitySnapshot> AddObservationAsync(ObservationData observation, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Write);
        if ((await GetAsync(observation.ArtifactId, ct)).Kind != "artifact") throw new ArgumentException("Source must be an artifact.");
        if (string.IsNullOrWhiteSpace(observation.Engine) || string.IsNullOrWhiteSpace(observation.EngineVersion) || observation.ConfigurationSha256.Length != 64)
            throw new ArgumentException("Processing profile is required.");
        return await InsertAsync("observation", observation, Sensitivity.Restricted, ct);
    }
    /// Newest-first keyset traversal. Appending revisions cannot shift an existing continuation.
    public async Task<RevisionPage> HistoryPageAsync(Guid id, int? beforeRevision = null, int limit = 25, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        if (beforeRevision is < 1 || limit is < 1 or > 100) throw new ArgumentException("Invalid history page.");
        var current = await MetadataAsync(id, ct);
        var query = db.Revisions.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.EntityId == id);
        if (beforeRevision is int before) query = query.Where(x => x.Revision < before);
        var revisions = await query.OrderByDescending(x => x.Revision).Take(limit + 1).ToListAsync(ct);
        var items = revisions.Take(limit).Select(x => RevisionSnapshot(current, x)).ToArray();
        return new(items, revisions.Count > limit ? items[^1].Revision : null);
    }
    /// Stable ordering within an unchanged journal. Offsets are not snapshot cursors: exports can append to older revisions.
    public async Task<GraphPage<AuditEvent>> AuditPageAsync(Guid id, int offset = 0, int limit = 25, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        if (offset < 0 || offset > int.MaxValue - 101 || limit is < 1 or > 100) throw new ArgumentException("Invalid audit page.");
        await MetadataAsync(id, ct);
        var events = await db.Audit.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.EntityId == id)
            .OrderByDescending(x => x.Revision).ThenBy(x => x.Id).Skip(offset).Take(limit + 1)
            .Select(x => new AuditEvent(x.Id, x.Tenant, x.Actor, x.EntityId, x.Revision, x.Action, x.At)).ToListAsync(ct);
        return new(events.Take(limit).ToArray(), events.Count > limit ? checked(offset + limit) : null);
    }
    /// Legacy ascending array, capped explicitly rather than silently truncated. Prefer HistoryPageAsync.
    public async Task<IReadOnlyList<EntitySnapshot>> HistoryAsync(Guid id, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        var current = await MetadataAsync(id, ct);
        var revisions = await db.Revisions.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.EntityId == id).OrderBy(x => x.Revision).Take(501).ToListAsync(ct);
        if (revisions.Count > 500) throw new InvalidOperationException("Use paged history for more than 500 revisions.");
        return revisions.Select(x => RevisionSnapshot(current, x)).ToArray();
    }
    /// Legacy ascending array, capped explicitly rather than silently truncated. Prefer AuditPageAsync.
    public async Task<IReadOnlyList<AuditEvent>> AuditAsync(Guid id, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        await MetadataAsync(id, ct);
        var events = await db.Audit.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.EntityId == id).OrderBy(x => x.Revision).ThenBy(x => x.Id).Take(501)
            .Select(x => new AuditEvent(x.Id, x.Tenant, x.Actor, x.EntityId, x.Revision, x.Action, x.At)).ToListAsync(ct);
        if (events.Count > 500) throw new InvalidOperationException("Use paged audit for more than 500 events.");
        return events;
    }
    async Task<GraphRecord> MetadataAsync(Guid id, CancellationToken ct) =>
        await db.Records.AsNoTracking().Where(x => x.Tenant == context.Tenant && x.Id == id)
            .Select(x => new GraphRecord { Id = x.Id, Kind = x.Kind, Sensitivity = x.Sensitivity }).SingleOrDefaultAsync(ct) ?? throw new GraphNotFoundException();
    EntitySnapshot RevisionSnapshot(GraphRecord current, GraphRevision revision) => new(current.Id, current.Kind, revision.Revision, current.Sensitivity,
        JsonSerializer.Deserialize<JsonElement>(cipher.Decrypt(revision.EnvelopeJson, Aad(context.Tenant, current.Kind, current.Id, revision.Revision, current.Sensitivity))));
    /// Resolve the one supported mutation author for a version; ambiguous/missing audit fails closed.
    public async Task<string> RevisionActorAsync(EntityVersion version, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read);
        await GetVersionAsync(version, ct);
        var actors = await db.Audit.AsNoTracking().Where(a => a.Tenant == context.Tenant && a.EntityId == version.Id && a.Revision == version.Revision &&
            (a.Action == "created" || a.Action == "revised")).Select(a => a.Actor).Take(2).ToArrayAsync(ct);
        return actors.Length == 1 ? actors[0] : throw new InvalidOperationException("Revision mutation author is missing or ambiguous.");
    }
    public async Task RecordExportAsync(Guid id, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read | GraphCapabilities.Export);
        var record = await GetAsync(id, ct);
        db.Audit.Add(new AuditRow { Id = Guid.NewGuid(), Tenant = context.Tenant, Actor = context.Actor, EntityId = id, Revision = record.Revision, Action = "exported", At = DateTimeOffset.UtcNow });
        await SaveAsync(ct);
    }
    /// Audit a bounded set of exact versions atomically before the caller releases an export.
    /// References are immutable; export delivery itself is outside the database transaction.
    public async Task RecordVersionExportsAsync(IReadOnlyList<EntityVersion> versions, CancellationToken ct = default)
    {
        context.Require(GraphCapabilities.Read | GraphCapabilities.Export);
        if (versions is null || versions.Count is < 1 or > 100 || versions.Any(v => v is null || v.Id == Guid.Empty || v.Revision < 1) || versions.Distinct().Count() != versions.Count)
            throw new ArgumentException("Invalid export references.");
        foreach (var version in versions) await GetVersionAsync(version, ct);
        var at = DateTimeOffset.UtcNow;
        db.Audit.AddRange(versions.Select(version => new AuditRow { Id = Guid.NewGuid(), Tenant = context.Tenant, Actor = context.Actor,
            EntityId = version.Id, Revision = version.Revision, Action = "exported", At = at }));
        await SaveAsync(ct);
    }
    EntitySnapshot Snapshot(GraphRecord record) => new(record.Id, record.Kind, record.Revision, record.Sensitivity,
        JsonSerializer.Deserialize<JsonElement>(cipher.Decrypt(record.EnvelopeJson, Aad(record.Tenant, record.Kind, record.Id, record.Revision, record.Sensitivity))));
    void QueueAudit(GraphRecord record, string action)
    {
        pendingAudits.Add(new AuditRow { Id = Guid.NewGuid(), Tenant = context.Tenant, Actor = context.Actor, EntityId = record.Id, Revision = record.Revision, Action = action, At = DateTimeOffset.UtcNow });
    }
    async Task SaveAsync(CancellationToken ct)
    {
        try { await GraphMigrator.RequireCurrentAsync(db, ct); }
        catch { pendingAudits.Clear(); db.ChangeTracker.Clear(); throw; }
        var audits = pendingAudits.ToArray(); pendingAudits.Clear();
        var outer = db.Database.CurrentTransaction;
        Microsoft.EntityFrameworkCore.Storage.IDbContextTransaction? started = null;
        try { if (outer is null) started = await db.Database.BeginTransactionAsync(ct); }
        catch { db.ChangeTracker.Clear(); throw; }
        await using var owned = started;
        var transaction = outer ?? owned!; var savepoint = "cg_" + Guid.NewGuid().ToString("N");
        if (outer is not null) await transaction.CreateSavepointAsync(savepoint, ct);
        try
        {
            // Database triggers create immutable snapshots before the corresponding audit insert.
            await db.SaveChangesAsync(ct);
            if (audits.Length > 0) { db.Audit.AddRange(audits); await db.SaveChangesAsync(ct); }
            if (owned is not null) await owned.CommitAsync(ct); else await transaction.ReleaseSavepointAsync(savepoint, ct);
        }
        catch (Exception error)
        {
            if (owned is not null) await owned.RollbackAsync(CancellationToken.None); else await transaction.RollbackToSavepointAsync(savepoint, CancellationToken.None);
            db.ChangeTracker.Clear();
            if (error is DbUpdateConcurrencyException || error is DbUpdateException { InnerException: Npgsql.PostgresException { SqlState: "23505", ConstraintName: "PK_Revisions" or "PK_Records" } } ||
                error is DbUpdateException { InnerException: Microsoft.Data.Sqlite.SqliteException { SqliteExtendedErrorCode: 1555 } } ||
                error is DbUpdateException { InnerException: Microsoft.Data.Sqlite.SqliteException { SqliteExtendedErrorCode: 1811 } sqlite } && sqlite.Message.Contains("casegraph identity conflict", StringComparison.Ordinal)) throw new GraphConflictException();
            throw;
        }
    }
}
