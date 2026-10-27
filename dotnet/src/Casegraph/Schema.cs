using System.Text.Json;
namespace Casegraph;

public sealed record FieldDefinition(string Name, JsonValueKind ValueKind, bool Required, Sensitivity Sensitivity,
    int? MaxLength = null, IReadOnlyList<string>? AllowedValues = null, bool NonBlank = false);
public sealed record EntitySchema(string EntityType, int Version, IReadOnlyList<FieldDefinition> Fields, bool PreserveUnknownFields = true);
public sealed record SchemaDiagnostic(string Code, string Field);
public static class SchemaValidator
{
    /// Does not erase unknown data or coerce types. Classification applies to fields in the schema.
    public static IReadOnlyList<SchemaDiagnostic> Validate(EntitySchema schema, JsonElement data)
    {
        ArgumentNullException.ThrowIfNull(schema);
        if (schema.Version < 1 || string.IsNullOrWhiteSpace(schema.EntityType) || schema.Fields is null || schema.Fields.Count > 100 ||
            schema.Fields.Any(f => f is null || string.IsNullOrWhiteSpace(f.Name) || f.Name.Length > 200 || !Enum.IsDefined(f.ValueKind) || f.ValueKind == JsonValueKind.Undefined || !Enum.IsDefined(f.Sensitivity)) ||
            schema.Fields.Select(f => f.Name).Distinct(StringComparer.Ordinal).Count() != schema.Fields.Count)
            throw new ArgumentException("Invalid schema.");
        foreach (var field in schema.Fields)
        {
            if (field.MaxLength is < 1 or > 10000 || (field.MaxLength is not null || field.AllowedValues is not null || field.NonBlank) && field.ValueKind != JsonValueKind.String ||
                field.AllowedValues is { } values && (values.Count is < 1 or > 100 || values.Any(v => v is null || v.Length > 10000 || field.MaxLength is int max && v.Length > max || field.NonBlank && string.IsNullOrWhiteSpace(v)) || values.Distinct(StringComparer.Ordinal).Count() != values.Count))
                throw new ArgumentException("Invalid string constraint.");
        }
        if (data.ValueKind != JsonValueKind.Object) return [new("OBJECT_REQUIRED", "")];
        var issues = new List<SchemaDiagnostic>();
        var names = new HashSet<string>(StringComparer.Ordinal);
        foreach (var property in data.EnumerateObject())
        {
            if (!names.Add(property.Name)) issues.Add(new("DUPLICATE_FIELD", property.Name));
            if (!schema.PreserveUnknownFields && !schema.Fields.Any(f => f.Name == property.Name)) issues.Add(new("UNKNOWN_FIELD", property.Name));
        }
        foreach (var field in schema.Fields)
        {
            if (!data.TryGetProperty(field.Name, out var value)) { if (field.Required) issues.Add(new("FIELD_REQUIRED", field.Name)); }
            else if (value.ValueKind != field.ValueKind) issues.Add(new("FIELD_TYPE", field.Name));
            else if (value.ValueKind == JsonValueKind.String)
            {
                var text = value.GetString()!;
                if (field.MaxLength is int max && text.Length > max) issues.Add(new("FIELD_LENGTH", field.Name));
                if (field.NonBlank && string.IsNullOrWhiteSpace(text)) issues.Add(new("FIELD_BLANK", field.Name));
                if (field.AllowedValues is { } values && !values.Contains(text, StringComparer.Ordinal)) issues.Add(new("FIELD_VALUE", field.Name));
            }
        }
        return issues;
    }
}
