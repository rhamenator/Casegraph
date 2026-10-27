using Casegraph;
namespace Casegraph.Tests;

public sealed class WorkflowTopologyTests
{
    [Fact]
    public void AnalysisHandlesCyclesDisconnectedStatesAndTerminalsWithoutDomainPolicy()
    {
        var definition = new WorkflowDefinition("Generic", 1, "Start", ["Start", "Loop", "End", "Unreachable"],
            [new("Start", "Loop", false), new("Loop", "Start", false), new("Loop", "End", false)]);
        var topology = GraphWorkflowService.AnalyzeDefinition(definition);
        Assert.Equal(new[] { "Unreachable" }, topology.UnreachableStates);
        Assert.Equal(new[] { "End", "Unreachable" }, topology.TerminalStates);
        GraphWorkflowService.ValidateTransition(definition, "Start", "Loop", false);
    }
    [Theory]
    [InlineData(" Start")]
    [InlineData("Start\nInjected")]
    [InlineData("")]
    public void InvalidStateLabelsAreRejected(string state) => Assert.Throws<ArgumentException>(() =>
        GraphWorkflowService.AnalyzeDefinition(new("Generic", 1, state, [state], [])));
    [Fact]
    public void MissingDefinitionAndDuplicateTransitionsAreRejected()
    {
        Assert.Throws<ArgumentException>(() => GraphWorkflowService.AnalyzeDefinition(null!));
        Assert.Throws<ArgumentException>(() => GraphWorkflowService.AnalyzeDefinition(new("Generic", 1, "A", ["A", "B"], [new("A", "B", false), new("A", "B", true)])));
    }
}
