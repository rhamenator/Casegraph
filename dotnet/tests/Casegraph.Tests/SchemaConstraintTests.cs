using System.Text.Json;
using Casegraph;
namespace Casegraph.Tests;

public sealed class SchemaConstraintTests
{
    [Fact]
    public void StringsEnforceBoundsBlankAndOrdinalChoicesWithoutCoercion()
    {
        var schema = new EntitySchema("synthetic", 1, [new("choice", JsonValueKind.String, true, Sensitivity.Restricted, 3, ["Yes", "No"], true)], false);
        Assert.Empty(SchemaValidator.Validate(schema, JsonSerializer.SerializeToElement(new { choice = "Yes" })));
        Assert.Contains(new SchemaDiagnostic("FIELD_REQUIRED", "choice"), SchemaValidator.Validate(schema, JsonSerializer.SerializeToElement(new { })));
        Assert.Contains(new SchemaDiagnostic("FIELD_TYPE", "choice"), SchemaValidator.Validate(schema, JsonSerializer.SerializeToElement(new { choice = 1 })));
        Assert.Contains(new SchemaDiagnostic("FIELD_BLANK", "choice"), SchemaValidator.Validate(schema, JsonSerializer.SerializeToElement(new { choice = " " })));
        Assert.Contains(new SchemaDiagnostic("FIELD_LENGTH", "choice"), SchemaValidator.Validate(schema, JsonSerializer.SerializeToElement(new { choice = "long" })));
        Assert.Contains(new SchemaDiagnostic("FIELD_VALUE", "choice"), SchemaValidator.Validate(schema, JsonSerializer.SerializeToElement(new { choice = "yes" })));
        var optional = schema with { Fields = [new("choice", JsonValueKind.String, false, Sensitivity.Restricted, 3)] };
        Assert.Empty(SchemaValidator.Validate(optional, JsonSerializer.SerializeToElement(new { choice = "" })));
    }
    [Fact]
    public void DuplicateInputIsDiagnosedAndUnknownDataIsPreserved()
    {
        using var input = JsonDocument.Parse("{\"name\":\"one\",\"name\":\"two\",\"extra\":true}");
        var schema = new EntitySchema("synthetic", 1, [new("name", JsonValueKind.String, true, Sensitivity.Internal)]);
        var issues = SchemaValidator.Validate(schema, input.RootElement); Assert.Contains(new SchemaDiagnostic("DUPLICATE_FIELD", "name"), issues);
        Assert.DoesNotContain(issues, x => x.Code == "UNKNOWN_FIELD"); Assert.True(input.RootElement.GetProperty("extra").GetBoolean());
        Assert.Contains(new SchemaDiagnostic("UNKNOWN_FIELD", "extra"), SchemaValidator.Validate(schema with { PreserveUnknownFields = false }, input.RootElement));
        Assert.Equal("OBJECT_REQUIRED", Assert.Single(SchemaValidator.Validate(schema, JsonSerializer.SerializeToElement(1))).Code);
    }
    [Fact]
    public void InvalidSchemasFailBeforeValidatingData()
    {
        FieldDefinition[] invalid = [new("", JsonValueKind.String, true, Sensitivity.Internal), new("x", JsonValueKind.Undefined, true, Sensitivity.Internal),
            new("x", JsonValueKind.String, true, (Sensitivity)99), new("x", JsonValueKind.String, true, Sensitivity.Internal, 0),
            new("x", JsonValueKind.Number, true, Sensitivity.Internal, NonBlank: true), new("x", JsonValueKind.String, true, Sensitivity.Internal, AllowedValues: []),
            new("x", JsonValueKind.String, true, Sensitivity.Internal, AllowedValues: ["a", "a"]), new("x", JsonValueKind.String, true, Sensitivity.Internal, 1, ["aa"]),
            new("x", JsonValueKind.String, true, Sensitivity.Internal, AllowedValues: [" "], NonBlank: true)];
        foreach (var field in invalid) Assert.Throws<ArgumentException>(() => SchemaValidator.Validate(new("synthetic", 1, [field]), JsonSerializer.SerializeToElement(new { })));
        Assert.Throws<ArgumentException>(() => SchemaValidator.Validate(new("synthetic", 1, [invalid[0], invalid[0]]), JsonSerializer.SerializeToElement(new { })));
    }
}
