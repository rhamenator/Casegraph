using System.Text.Json;
namespace Casegraph;

public sealed record WorkflowTransition(string From, string To, bool RequiresHumanReview);
public sealed record WorkflowDefinition(string Name, int SchemaVersion, string InitialState, string[] States, WorkflowTransition[] Transitions);
public sealed record WorkflowTopology(IReadOnlyList<string> UnreachableStates, IReadOnlyList<string> TerminalStates);
public sealed record WorkflowInstance(Guid SubjectId, Guid DefinitionId, string State);

/// Explicit version-pinned state machine. No automatic evaluation or hiring decisions.
public sealed class GraphWorkflowService(GraphStore graph)
{
    public static void ValidateDefinition(WorkflowDefinition definition)
    {
        if (definition is null || string.IsNullOrWhiteSpace(definition.Name) || definition.Name.Length > 200 || definition.Name.Any(char.IsControl) || definition.SchemaVersion < 1 || (definition.States is null || definition.Transitions is null) || definition.States.Length is 0 or > 100 || definition.States.Any(s => string.IsNullOrWhiteSpace(s) || s.Length > 80 || s.Any(char.IsControl) || s != s.Trim()) ||
            definition.States.Distinct().Count() != definition.States.Length || !definition.States.Contains(definition.InitialState) ||
            definition.Transitions.Length > 500 || definition.Transitions.Any(t => t is null || !definition.States.Contains(t.From) || !definition.States.Contains(t.To) || t.From == t.To) ||
            definition.Transitions.Select(t => (t.From, t.To)).Distinct().Count() != definition.Transitions.Length)
            throw new ArgumentException("Invalid workflow definition.");
    }
    /// Describe graph reachability without imposing a domain-specific terminal/review policy.
    public static WorkflowTopology AnalyzeDefinition(WorkflowDefinition definition)
    {
        ValidateDefinition(definition);
        var reached = new HashSet<string> { definition.InitialState };
        var pending = new Queue<string>(); pending.Enqueue(definition.InitialState);
        while (pending.TryDequeue(out var state))
            foreach (var transition in definition.Transitions.Where(t => t.From == state))
                if (reached.Add(transition.To)) pending.Enqueue(transition.To);
        return new(definition.States.Where(s => !reached.Contains(s)).ToArray(),
            definition.States.Where(s => !definition.Transitions.Any(t => t.From == s)).ToArray());
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
        ValidateDefinition(definition);
        return await graph.CreateAsync("workflow.instance", new WorkflowInstance(subjectId, definitionId, definition.InitialState), ct: ct);
    }
    public async Task<EntitySnapshot> MoveAsync(Guid instanceId, int expectedRevision, string nextState, bool humanReviewRecorded, CancellationToken ct = default)
    {
        var entity = await graph.GetAsync(instanceId, ct);
        if (entity.Kind != "workflow.instance") throw new ArgumentException("Workflow instance required.");
        var instance = entity.Data.Deserialize<WorkflowInstance>()!;
        var definitionEntity = await graph.GetAsync(instance.DefinitionId, ct);
        if (definitionEntity.Kind != "workflow.definition") throw new ArgumentException("Workflow definition required.");
        var definition = definitionEntity.Data.Deserialize<WorkflowDefinition>()!;
        ValidateTransition(definition, instance.State, nextState, humanReviewRecorded);
        return await graph.ReviseAsync(instanceId, expectedRevision, instance with { State = nextState }, ct);
    }
}
