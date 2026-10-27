using System.Text.Json;
namespace Casegraph;

public sealed record FieldDefinition(string Name, JsonValueKind ValueKind, bool Required, Sensitivity Sensitivity);
public sealed record EntitySchema(string EntityType, int Version, IReadOnlyList<FieldDefinition> Fields, bool PreserveUnknownFields = true);
public sealed record SchemaDiagnostic(string Code, string Field);
public static class SchemaValidator
{
    /// Does not erase unknown data or coerce types. Classification applies to fields in the schema.
    public static IReadOnlyList<SchemaDiagnostic> Validate(EntitySchema schema, JsonElement data)
    {
        if (schema.Version < 1 || string.IsNullOrWhiteSpace(schema.EntityType) || schema.Fields.Select(f => f.Name).Distinct().Count() != schema.Fields.Count)
            throw new ArgumentException("Invalid schema.");
        if (data.ValueKind != JsonValueKind.Object) return [new("OBJECT_REQUIRED", "")];
        var issues = new List<SchemaDiagnostic>();
        foreach (var field in schema.Fields)
        {
            if (!data.TryGetProperty(field.Name, out var value)) { if (field.Required) issues.Add(new("FIELD_REQUIRED", field.Name)); }
            else if (value.ValueKind != field.ValueKind) issues.Add(new("FIELD_TYPE", field.Name));
        }
        if (!schema.PreserveUnknownFields)
            foreach (var property in data.EnumerateObject()) if (!schema.Fields.Any(f => f.Name == property.Name)) issues.Add(new("UNKNOWN_FIELD", property.Name));
        return issues;
    }
}
