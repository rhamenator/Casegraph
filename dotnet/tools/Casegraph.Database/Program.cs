using System.Text.Json;
using Casegraph;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;

try
{
    var command = args.SingleOrDefault() ?? "plan";
    var provider = Environment.GetEnvironmentVariable("CASEGRAPH_PROVIDER") ?? throw new InvalidOperationException("Set CASEGRAPH_PROVIDER.");
    if (provider is not ("sqlite" or "postgres")) throw new InvalidOperationException("Use sqlite or postgres.");
    if (command == "scripts") { foreach (var migration in GraphMigrator.Scripts(provider)) Console.WriteLine($"-- version {migration.Version} sha256 {migration.Sha256}\n{migration.Sql}"); return; }
    if (command == "runtime-grants") { Console.WriteLine(GraphMigrator.RuntimeGrants(Environment.GetEnvironmentVariable("CASEGRAPH_RUNTIME_ROLE") ?? throw new InvalidOperationException("Set CASEGRAPH_RUNTIME_ROLE."))); return; }
    if (command is not ("plan" or "apply" or "adopt-legacy")) throw new ArgumentException("Use plan, scripts, apply, adopt-legacy, or runtime-grants.");
    var connection = Environment.GetEnvironmentVariable("CASEGRAPH_CONNECTION") ?? throw new InvalidOperationException("Set CASEGRAPH_CONNECTION externally.");
    try
    {
        if (provider == "sqlite" && command == "plan")
        {
            var settings = new SqliteConnectionStringBuilder(connection);
            if (settings.DataSource != ":memory:" && !File.Exists(settings.DataSource))
            { Console.WriteLine(JsonSerializer.Serialize(new MigrationStatus("sqlite", 0, false, [1, 2]))); return; }
            settings.Mode = SqliteOpenMode.ReadOnly; connection = settings.ToString();
        }
        var options = new DbContextOptionsBuilder<GraphDb>();
        if (provider == "postgres") options.UseNpgsql(connection); else options.UseSqlite(connection);
        await using var db = new GraphDb(options.Options);
        if (command != "plan") await GraphMigrator.ApplyAsync(db, adoptLegacy: command == "adopt-legacy");
        Console.WriteLine(JsonSerializer.Serialize(await GraphMigrator.StatusAsync(db)));
    }
    catch (GraphMigrationException error) { Console.Error.WriteLine(error.Message); Environment.ExitCode = 1; }
    catch { Console.Error.WriteLine("Database operation failed. Connection details and database values are omitted."); Environment.ExitCode = 1; }

}
catch { Console.Error.WriteLine("Use one valid database command and configure CASEGRAPH_PROVIDER plus CASEGRAPH_CONNECTION (or CASEGRAPH_RUNTIME_ROLE for grants). No connection values are printed."); Environment.ExitCode = 1; }
