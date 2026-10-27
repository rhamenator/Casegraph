using System.Text.Json;
namespace Casegraph;

public sealed record WorkflowTransition(string From, string To, bool RequiresHumanReview);
public sealed record WorkflowDefinition(string Name, int SchemaVersion, string InitialState, string[] States, WorkflowTransition[] Transitions);
public sealed record WorkflowInstance(Guid SubjectId, Guid DefinitionId, string State);

/// Explicit version-pinned state machine. No automatic evaluation or hiring decisions.
public sealed class GraphWorkflowService(GraphStore graph)
{
    public static void ValidateDefinition(WorkflowDefinition definition)
    {
        if (string.IsNullOrWhiteSpace(definition.Name) || definition.SchemaVersion < 1 || (definition.States is null || definition.Transitions is null) || definition.States.Length is 0 or > 100 || definition.States.Any(s => string.IsNullOrWhiteSpace(s) || s.Length > 80) ||
            definition.States.Distinct().Count() != definition.States.Length || !definition.States.Contains(definition.InitialState) ||
            definition.Transitions.Length > 500 || definition.Transitions.Any(t => t is null || !definition.States.Contains(t.From) || !definition.States.Contains(t.To) || t.From == t.To) ||
            definition.Transitions.Select(t => (t.From, t.To)).Distinct().Count() != definition.Transitions.Length)
            throw new ArgumentException("Invalid workflow definition.");
    }
    public static void ValidateTransition(WorkflowDefinition definition, string currentState, string nextState, bool humanReviewRecorded)
    {
        ValidateDefinition(definition);
        var transition = definition.Transitions.SingleOrDefault(t => t.From == currentState && t.To == nextState);
        if (transition is null || transition.RequiresHumanReview && !humanReviewRecorded) throw new ArgumentException("Workflow transition is not allowed.");
    }
    public async Task<EntitySnapshot> DefineAsync(WorkflowDefinition definition, CancellationToken ct = default)
    {
        ValidateDefinition(definition);
        return await graph.CreateAsync("workflow.definition", definition, ct: ct);
    }
    public async Task<EntitySnapshot> StartAsync(Guid subjectId, Guid definitionId, CancellationToken ct = default)
    {
        await graph.GetAsync(subjectId, ct);
        var entity = await graph.GetAsync(definitionId, ct);
        if (entity.Kind != "workflow.definition") throw new ArgumentException("Workflow definition required.");
        var definition = entity.Data.Deserialize<WorkflowDefinition>()!;
        return await graph.CreateAsync("workflow.instance", new WorkflowInstance(subjectId, definitionId, definition.InitialState), ct: ct);
    }
    public async Task<EntitySnapshot> MoveAsync(Guid instanceId, int expectedRevision, string nextState, bool humanReviewRecorded, CancellationToken ct = default)
    {
        var entity = await graph.GetAsync(instanceId, ct);
        if (entity.Kind != "workflow.instance") throw new ArgumentException("Workflow instance required.");
        var instance = entity.Data.Deserialize<WorkflowInstance>()!;
        var definitionEntity = await graph.GetAsync(instance.DefinitionId, ct);
        var definition = definitionEntity.Data.Deserialize<WorkflowDefinition>()!;
        ValidateTransition(definition, instance.State, nextState, humanReviewRecorded);
        return await graph.ReviseAsync(instanceId, expectedRevision, instance with { State = nextState }, ct);
    }
}
