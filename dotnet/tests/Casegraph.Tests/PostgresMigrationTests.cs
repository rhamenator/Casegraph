using Casegraph;
using Microsoft.EntityFrameworkCore;
using Npgsql;
namespace Casegraph.Tests;

[Collection("PostgreSQL")]
public sealed class PostgresMigrationTests
{
    [PostgresFact]
    public async Task LegacyUpgradeAndRestrictedRuntimeRoleEnforceDatabaseProtections()
    {
        var setting = Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES")!;
        await using var admin = new NpgsqlConnection(setting); await admin.OpenAsync();
        var database = "cg_legacy_" + Guid.NewGuid().ToString("N"); var role = "cg_runtime_" + Guid.NewGuid().ToString("N");
        async Task Sql(NpgsqlConnection connection, string text) { await using var cmd = new NpgsqlCommand(text, connection); await cmd.ExecuteNonQueryAsync(); }
        await Sql(admin, $"CREATE DATABASE \"{database}\"");
        var builder = new NpgsqlConnectionStringBuilder(setting) { Database = database, Pooling = false };
        try
        {
            await using (var legacy = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseNpgsql(builder.ToString()).Options))
            {
                var id = await LegacyFixture.SeedAsync(legacy);
                await Assert.ThrowsAsync<GraphMigrationException>(() => GraphMigrator.ApplyAsync(legacy));
                await GraphMigrator.ApplyAsync(legacy, true); await GraphMigrator.ApplyAsync(legacy);
                var graph = new GraphStore(legacy, LegacyFixture.Cipher(), new("legacy", "synthetic")); Assert.Equal(2, (await graph.HistoryAsync(id)).Count);
                await graph.ReviseAsync(id, 2, new { Value = "synthetic after upgrade" }); Assert.Equal(3, (await graph.HistoryAsync(id)).Count);
            }
            await using var connection = new NpgsqlConnection(builder.ToString()); await connection.OpenAsync(); await Sql(connection, $"CREATE ROLE \"{role}\" NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE");
            await Sql(connection, GraphMigrator.RuntimeGrants(role)); await Sql(connection, $"SET ROLE \"{role}\"");
            try
            {
                await using var runtime = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseNpgsql(connection).Options);
                var graph = new GraphStore(runtime, LegacyFixture.Cipher(), new("runtime", "synthetic"));
                var entity = await graph.CreateAsync("person", new { Value = "synthetic runtime" }); await graph.ReviseAsync(entity.Id, 1, new { Value = "synthetic update" });
                var artifact = await graph.AddArtifactAsync("synthetic source"u8.ToArray(), "text/plain"); Assert.Equal(2, (await graph.HistoryAsync(entity.Id)).Count);
                foreach (var sql in new[] { "UPDATE \"Records\" SET \"Revision\"=\"Revision\"+1 WHERE \"Kind\"='artifact'", "DELETE FROM \"Revisions\"", "UPDATE \"Audit\" SET \"Actor\"='tampered'", "TRUNCATE \"Records\" CASCADE", "ALTER TABLE \"Records\" DISABLE TRIGGER ALL", "DROP TRIGGER cg_records_guard ON \"Records\"", "INSERT INTO \"CasegraphMigrations\" VALUES (999,'bad','bad','bad')" })
                    await Assert.ThrowsAsync<PostgresException>(() => Sql(connection, sql));
                Assert.Equal("artifact", (await graph.GetAsync(artifact.Id)).Kind);
            }
            finally { await Sql(connection, "RESET ROLE"); await Sql(connection, $"DROP OWNED BY \"{role}\"; DROP ROLE \"{role}\""); }
        }
        finally { await Sql(admin, $"DROP DATABASE \"{database}\" WITH (FORCE)"); }
    }
    [PostgresFact]
    public async Task DirectOwnerSqlStillCannotUpdateHistoryOrTruncateWithoutDisablingGuards()
    {
        await using var db = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseNpgsql(Environment.GetEnvironmentVariable("CASEGRAPH_TEST_POSTGRES")).Options); await GraphMigrator.ApplyAsync(db);
        foreach (var sql in new[] { "UPDATE \"Revisions\" SET \"EnvelopeJson\"='{}'", "DELETE FROM \"Audit\"", "TRUNCATE \"Records\" CASCADE" })
        {
            // Ensure a row exists so row-level guards execute even on a newly created database.
            var graph = new GraphStore(db, LegacyFixture.Cipher(), new("owner-test", "synthetic")); await graph.CreateAsync("person", new { Value = "synthetic" });
            await Assert.ThrowsAsync<PostgresException>(() => db.Database.ExecuteSqlRawAsync(sql.Replace("{}", "{{}}")));
        }
    }
}
