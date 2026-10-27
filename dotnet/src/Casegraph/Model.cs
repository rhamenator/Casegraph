using System.Text.Json;
namespace Casegraph;

public enum Sensitivity { Public, Internal, Confidential, Restricted }
[Flags]
public enum GraphCapabilities { None = 0, Read = 1, Write = 2, Export = 4, All = Read | Write | Export }
public sealed class GraphAccessDeniedException() : Exception("Graph access denied.");
public sealed record GraphContext
{
    public string Tenant { get; }
    public string Actor { get; }
    public GraphCapabilities Capabilities { get; }
    public void Require(GraphCapabilities capability) { if ((Capabilities & capability) != capability) throw new GraphAccessDeniedException(); }
    public GraphContext(string tenant, string actor, GraphCapabilities capabilities = GraphCapabilities.All)
    {
        if (!Valid(tenant) || !Valid(actor) || (capabilities & ~GraphCapabilities.All) != 0) throw new ArgumentException("Invalid security context.");
        Tenant = tenant; Actor = actor; Capabilities = capabilities;
    }
    static bool Valid(string value) => !string.IsNullOrWhiteSpace(value) && value.Length <= 100 && value.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or ':' or '.');
}
public sealed record GraphPage<T>(IReadOnlyList<T> Items, int? NextOffset);
public sealed record RevisionPage(IReadOnlyList<EntitySnapshot> Items, int? NextBeforeRevision);
public sealed record EntityVersion(Guid Id, int Revision);
public sealed record EntitySnapshot(Guid Id, string Kind, int Revision, Sensitivity Sensitivity, JsonElement Data);
public sealed record RelationshipData(Guid From, string Type, Guid To);
public sealed record ArtifactData(string Sha256, string MediaType, string BytesBase64);
public sealed record ObservationData(Guid ArtifactId, string Engine, string EngineVersion, string ConfigurationSha256, JsonElement Output);
public sealed record AuditEvent(Guid Id, string Tenant, string Actor, Guid EntityId, int Revision, string Action, DateTimeOffset At);
public sealed class GraphConflictException() : Exception("The record changed. Reload before saving.");
public sealed class GraphNotFoundException() : Exception("Record not found.");
public sealed class GraphRecord
{
    public string Tenant { get; set; } = "";
    public Guid Id { get; set; }
    public string Kind { get; set; } = "";
    public int Revision { get; set; }
    public Sensitivity Sensitivity { get; set; }
    public string EnvelopeJson { get; set; } = "";
}
public sealed class GraphRevision
{
    public string Tenant { get; set; } = "";
    public Guid EntityId { get; set; }
    public int Revision { get; set; }
    public string EnvelopeJson { get; set; } = "";
}
public sealed class AuditRow
{
    public Guid Id { get; set; }
    public string Tenant { get; set; } = "";
    public string Actor { get; set; } = "";
    public Guid EntityId { get; set; }
    public int Revision { get; set; }
    public string Action { get; set; } = "";
    public DateTimeOffset At { get; set; }
}
