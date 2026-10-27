using System.Text.Json;
namespace Casegraph;

public sealed record DefinitionData(string Type, int SchemaVersion, string Family, string Name, Guid RootId, int Version,
    EntityVersion? DerivedFrom, JsonElement Content, string Actor, DateTimeOffset At);

/// Explicit immutable versions. Branches are permitted; there is no inferred mutable latest head.
public sealed class GraphDefinitionService(GraphStore graph, TimeProvider time)
{
    public static DefinitionData Read(EntitySnapshot entity)
    {
        if (entity.Kind != "receipt" || !entity.Data.TryGetProperty("Type", out var type) || type.GetString() != "casegraph.definition") throw new GraphNotFoundException();
        var data = entity.Data.Deserialize<DefinitionData>()!;
        if (data.SchemaVersion != 1) throw new ArgumentException("Unsupported definition schema.");
        return data;
    }
    public async Task<DefinitionData> GetAsync(Guid id, CancellationToken ct = default) => Read(await graph.GetVersionAsync(new(id, 1), ct));
    public async Task<DefinitionData> DefineAsync(Guid requestId, string family, string name, JsonElement content, Guid? derivedFrom = null, CancellationToken ct = default)
    {
        graph.Context.Require(GraphCapabilities.Read | GraphCapabilities.Write);
        if (requestId == Guid.Empty || derivedFrom == requestId || string.IsNullOrWhiteSpace(family) || family.Length > 80 || !family.All(c => char.IsAsciiLetterOrDigit(c) || c is '.' or '-') ||
            string.IsNullOrWhiteSpace(name) || name.Length > 200 || name.Any(char.IsControl) || content.ValueKind != JsonValueKind.Object || JsonSerializer.SerializeToUtf8Bytes(content).Length > 65536)
            throw new ArgumentException("Invalid definition.");
        var previous = derivedFrom is Guid parentId ? new EntityVersion(parentId, 1) : null;
        EntitySnapshot? existing = null;
        try { existing = await graph.GetAsync(requestId, ct); } catch (GraphNotFoundException) { }
        if (existing is not null)
        {
            DefinitionData data;
            try { data = Read(existing); } catch (GraphNotFoundException) { throw new GraphConflictException(); }
            if (data.Family != family || data.Name != name || data.DerivedFrom != previous || data.Actor != graph.Context.Actor || !JsonElement.DeepEquals(data.Content, content)) throw new GraphConflictException();
            return data;
        }
        var parent = derivedFrom is Guid id ? await GetAsync(id, ct) : null;
        if (parent is not null && (parent.Family != family || parent.Version is < 1 or >= 1000)) throw new ArgumentException("Invalid definition lineage.");
        var definition = new DefinitionData("casegraph.definition", 1, family, name, parent?.RootId ?? requestId, (parent?.Version ?? 0) + 1, previous, content.Clone(), graph.Context.Actor, time.GetUtcNow());
        await graph.CreateWithIdAsync(requestId, "receipt", definition, ct: ct); return definition;
    }
}
