using System.Data;
using System.Data.Common;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Storage;
namespace Casegraph;

public sealed class GraphMigrationException(string message) : Exception(message);
public sealed record GraphMigration(int Version, string Name, string Sha256, string Sql);
public sealed record MigrationStatus(string Provider, int CurrentVersion, bool Legacy, IReadOnlyList<int> Pending);
/// Forward-only provider-specific migrations. No DDL runs from inspection or runtime readiness checks.
public static class GraphMigrator
{
    public const int CurrentVersion = 2;
    static string Provider(GraphDb db) => db.Database.ProviderName switch
    {
        "Microsoft.EntityFrameworkCore.Sqlite" => "sqlite",
        "Npgsql.EntityFrameworkCore.PostgreSQL" => "postgres",
        _ => throw new GraphMigrationException("Unsupported database provider.")
    };
    public static IReadOnlyList<GraphMigration> Scripts(string provider)
    {
        if (provider is not ("sqlite" or "postgres")) throw new GraphMigrationException("Unsupported database provider.");
        return new[] { (1, "base"), (2, "protection") }.Select(item =>
        {
            var name = $"{item.Item1:D3}-{item.Item2}.{provider}.sql";
            using var stream = typeof(GraphMigrator).Assembly.GetManifestResourceStream($"Casegraph.Migrations.{name}")!;
            using var reader = new StreamReader(stream); var sql = reader.ReadToEnd();
            return new GraphMigration(item.Item1, item.Item2, Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(sql))), sql);
        }).ToArray();
    }
    static async Task<List<string[]>> Rows(DbConnection connection, DbTransaction? transaction, string sql, CancellationToken ct, params (string, object)[] parameters)
    {
        await using var command = connection.CreateCommand(); command.Transaction = transaction; command.CommandText = sql; command.CommandTimeout = 30;
        foreach (var (key, value) in parameters) { var parameter = command.CreateParameter(); parameter.ParameterName = key; parameter.Value = value; command.Parameters.Add(parameter); }
        var rows = new List<string[]>(); await using var reader = await command.ExecuteReaderAsync(ct);
        while (await reader.ReadAsync(ct)) rows.Add(Enumerable.Range(0, reader.FieldCount).Select(i => reader.IsDBNull(i) ? "" : Convert.ToString(reader.GetValue(i), System.Globalization.CultureInfo.InvariantCulture)!).ToArray());
        return rows;
    }
    static async Task Execute(DbConnection connection, DbTransaction? transaction, string sql, CancellationToken ct)
    { await using var command = connection.CreateCommand(); command.Transaction = transaction; command.CommandText = sql; command.CommandTimeout = 30; await command.ExecuteNonQueryAsync(ct); }
    static async Task<MigrationStatus> Inspect(DbConnection connection, DbTransaction? tx, string provider, CancellationToken ct)
    {
        var tables = await Rows(connection, tx, provider == "sqlite" ? "SELECT name FROM sqlite_master WHERE type='table'" : "SELECT table_name FROM information_schema.tables WHERE table_schema='public'", ct);
        var names = tables.Select(r => r[0]).ToHashSet(); var coreCount = new[] { "Records", "Revisions", "Audit" }.Count(names.Contains);
        if (coreCount is > 0 and < 3 || coreCount == 0 && names.Contains("CasegraphMigrations")) throw new GraphMigrationException("Incomplete Casegraph schema.");
        var scripts = Scripts(provider); var current = 0;
        if (names.Contains("CasegraphMigrations"))
        {
            var applied = await Rows(connection, tx, "SELECT \"Version\",\"Name\",\"Sha256\" FROM \"CasegraphMigrations\" ORDER BY \"Version\"", ct);
            foreach (var row in applied)
            {
                if (!int.TryParse(row[0], out var version) || version != current + 1 || version > CurrentVersion || scripts[version - 1].Name != row[1] || scripts[version - 1].Sha256 != row[2])
                    throw new GraphMigrationException("Migration history is unknown, incomplete or has a checksum mismatch.");
                current = version;
            }
            if (current == 0) throw new GraphMigrationException("Empty migration history requires operator repair.");
        }
        return new(provider, current, coreCount == 3 && current == 0, scripts.Where(s => s.Version > current).Select(s => s.Version).ToArray());
    }
    public static async Task<MigrationStatus> StatusAsync(GraphDb db, CancellationToken ct = default)
    {
        var connection = db.Database.GetDbConnection(); var close = connection.State == ConnectionState.Closed; if (close) await connection.OpenAsync(ct);
        try { return await Inspect(connection, db.Database.CurrentTransaction?.GetDbTransaction(), Provider(db), ct); } finally { if (close) await connection.CloseAsync(); }
    }
    public static async Task RequireCurrentAsync(GraphDb db, CancellationToken ct = default)
    {
        if (db.SchemaValidated) return;
        var status = await StatusAsync(db, ct);
        if (status.CurrentVersion != CurrentVersion) throw new GraphMigrationException("Database migration is required. Use the database deployment tool.");
        var connection = db.Database.GetDbConnection(); var close = connection.State == ConnectionState.Closed; if (close) await connection.OpenAsync(ct);
        try
        {
            var active = db.Database.CurrentTransaction?.GetDbTransaction();
            var names = status.Provider == "sqlite"
                ? (await Rows(connection, active, "SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'cg_%'", ct)).Select(r => r[0]).ToHashSet()
                : (await Rows(connection, active, "SELECT tgname FROM pg_trigger WHERE NOT tgisinternal AND tgrelid IN ('public.\"Records\"'::regclass,'public.\"Revisions\"'::regclass,'public.\"Audit\"'::regclass)", ct)).Select(r => r[0]).ToHashSet();
            var expected = status.Provider == "sqlite" ? new[] { "cg_records_insert", "cg_records_update", "cg_records_delete", "cg_records_history_insert", "cg_records_history_update", "cg_revisions_insert", "cg_revisions_update", "cg_revisions_delete", "cg_audit_insert", "cg_audit_update", "cg_audit_delete" }
                : ["cg_records_guard", "cg_records_snapshot", "cg_revisions_guard", "cg_audit_guard", "cg_records_truncate", "cg_revisions_truncate", "cg_audit_truncate"];
            if (!expected.All(names.Contains)) throw new GraphMigrationException("Database evidence protections are missing.");
            db.SchemaValidated = true;
        }
        finally { if (close) await connection.CloseAsync(); }
    }
    public static async Task ApplyAsync(GraphDb db, bool adoptLegacy = false, CancellationToken ct = default)
    {
        db.SchemaValidated = false;
        if (db.Database.CurrentTransaction is not null) throw new GraphMigrationException("Migrations require a dedicated deployment connection.");
        var provider = Provider(db); var connection = db.Database.GetDbConnection(); var close = connection.State == ConnectionState.Closed; if (close) await connection.OpenAsync(ct);
        try
        {
            await using DbTransaction tx = connection is SqliteConnection sqlite ? sqlite.BeginTransaction(deferred: false) : await connection.BeginTransactionAsync(ct);
            try
            {
                if (provider == "postgres") await Execute(connection, tx, "SELECT pg_advisory_xact_lock(6820241027); SET LOCAL search_path=public,pg_catalog;", ct);
                var status = await Inspect(connection, tx, provider, ct);
                if (status.Legacy && !adoptLegacy) throw new GraphMigrationException("Legacy database adoption must be explicitly requested after backup.");
                if (status.Legacy || status.CurrentVersion > 0) await ValidateShape(connection, tx, provider, ct);
                if (status.Legacy || status.CurrentVersion == 1) await ValidateHistory(connection, tx, ct);
                if (status.CurrentVersion == 0) await Execute(connection, tx, "CREATE TABLE \"CasegraphMigrations\" (\"Version\" INTEGER PRIMARY KEY,\"Name\" TEXT NOT NULL,\"Sha256\" TEXT NOT NULL,\"AppliedAt\" TEXT NOT NULL)", ct);
                foreach (var migration in Scripts(provider).Where(s => s.Version > status.CurrentVersion))
                {
                    if (!(status.Legacy && migration.Version == 1)) await Execute(connection, tx, migration.Sql, ct);
                    await Rows(connection, tx, "INSERT INTO \"CasegraphMigrations\" (\"Version\",\"Name\",\"Sha256\",\"AppliedAt\") VALUES (@v,@n,@s,@a) RETURNING \"Version\"", ct,
                        ("v", migration.Version), ("n", migration.Name), ("s", migration.Sha256), ("a", DateTimeOffset.UtcNow.ToString("O")));
                }
                await tx.CommitAsync(ct);
            }
            catch { await tx.RollbackAsync(CancellationToken.None); throw; }
        }
        finally { if (close) await connection.CloseAsync(); }
        await RequireCurrentAsync(db, ct);
    }
    public static string RuntimeGrants(string role)
    {
        if (role.Length is < 1 or > 63 || !char.IsAsciiLetter(role[0]) || !role.All(c => char.IsAsciiLetterOrDigit(c) || c == '_'))
            throw new ArgumentException("Invalid database role identifier.");
        var name = "\"" + role + "\"";
        return $"REVOKE CREATE ON SCHEMA public FROM PUBLIC, {name};\nGRANT USAGE ON SCHEMA public TO {name};\n" +
            $"REVOKE ALL ON public.\"Records\",public.\"Revisions\",public.\"Audit\",public.\"CasegraphMigrations\" FROM {name};\n" +
            $"GRANT SELECT,INSERT,UPDATE ON public.\"Records\" TO {name};\nGRANT SELECT ON public.\"Revisions\",public.\"CasegraphMigrations\" TO {name};\nGRANT SELECT,INSERT ON public.\"Audit\" TO {name};\n" +
            $"REVOKE ALL ON FUNCTION public.cg_record_guard(),public.cg_record_snapshot(),public.cg_revision_guard(),public.cg_audit_guard(),public.cg_no_truncate() FROM {name};\n";
    }
    static async Task ValidateShape(DbConnection connection, DbTransaction tx, string provider, CancellationToken ct)
    {
        var id = provider == "sqlite" ? "TEXT" : "uuid"; var number = provider == "sqlite" ? "INTEGER" : "int4"; var text = provider == "sqlite" ? "TEXT" : "text";
        var expected = new Dictionary<string, Dictionary<string, string>>
        {
            ["Records"] = new() { ["Tenant"] = text, ["Id"] = id, ["Kind"] = text, ["Revision"] = number, ["Sensitivity"] = number, ["EnvelopeJson"] = provider == "sqlite" ? "TEXT" : "jsonb" },
            ["Revisions"] = new() { ["Tenant"] = text, ["EntityId"] = id, ["Revision"] = number, ["EnvelopeJson"] = provider == "sqlite" ? "TEXT" : "jsonb" },
            ["Audit"] = new() { ["Id"] = id, ["Tenant"] = text, ["Actor"] = text, ["EntityId"] = id, ["Revision"] = number, ["Action"] = text, ["At"] = provider == "sqlite" ? "TEXT" : "timestamptz" }
        };
        foreach (var (table, columns) in expected)
        {
            var rows = provider == "sqlite" ? await Rows(connection, tx, $"PRAGMA table_info(\"{table}\")", ct)
                : await Rows(connection, tx, "SELECT column_name,udt_name,is_nullable FROM information_schema.columns WHERE table_schema='public' AND table_name=@t", ct, ("t", table));
            var actual = rows.ToDictionary(r => provider == "sqlite" ? r[1] : r[0], r => provider == "sqlite" ? r[2] : r[1]);
            if (actual.Count != columns.Count || columns.Any(c => !actual.TryGetValue(c.Key, out var value) || value != c.Value) || rows.Any(r => provider == "sqlite" ? r[3] != "1" : r[2] != "NO"))
                throw new GraphMigrationException("Unsupported legacy table shape.");
            string[] keys = table == "Records" ? ["Tenant", "Id"] : table == "Revisions" ? ["Tenant", "EntityId", "Revision"] : ["Id"];
            var actualKeys = provider == "sqlite" ? rows.Where(r => r[5] != "0").OrderBy(r => int.Parse(r[5])).Select(r => r[1]).ToArray()
                : (await Rows(connection, tx, "SELECT k.column_name FROM information_schema.table_constraints c JOIN information_schema.key_column_usage k ON c.constraint_name=k.constraint_name AND c.table_schema=k.table_schema WHERE c.table_schema='public' AND c.table_name=@t AND c.constraint_type='PRIMARY KEY' ORDER BY k.ordinal_position", ct, ("t", table))).Select(r => r[0]).ToArray();
            if (!keys.SequenceEqual(actualKeys)) throw new GraphMigrationException("Unsupported legacy primary key.");
            if (table != "Records")
            {
                var foreign = provider == "sqlite" ? await Rows(connection, tx, $"PRAGMA foreign_key_list(\"{table}\")", ct)
                    : await Rows(connection, tx, "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid=@t::regclass AND contype='f'", ct, ("t", $"public.\"{table}\""));
                var valid = provider == "sqlite" ? foreign.Count == 2 && foreign.All(r => r[2] == "Records" && r[6] == "RESTRICT") && foreign.Any(r => r[3] == "Tenant" && r[4] == "Tenant") && foreign.Any(r => r[3] == "EntityId" && r[4] == "Id")
                    : foreign.Count == 1 && foreign[0][0].Contains("FOREIGN KEY (\"Tenant\", \"EntityId\") REFERENCES ") && foreign[0][0].Contains("\"Records\"(\"Tenant\", \"Id\") ON DELETE RESTRICT");
                if (!valid) throw new GraphMigrationException("Unsupported legacy foreign key.");
            }
        }
    }
    static async Task ValidateHistory(DbConnection connection, DbTransaction tx, CancellationToken ct)
    {
        var checks = new[] {
            "SELECT COUNT(*) FROM \"Records\" r LEFT JOIN \"Revisions\" h ON r.\"Tenant\"=h.\"Tenant\" AND r.\"Id\"=h.\"EntityId\" AND r.\"Revision\"=h.\"Revision\" WHERE h.\"EntityId\" IS NULL OR r.\"EnvelopeJson\"<>h.\"EnvelopeJson\"",
            "SELECT COUNT(*) FROM \"Records\" r WHERE r.\"Revision\"<1 OR r.\"Revision\"<>(SELECT COUNT(*) FROM \"Revisions\" h WHERE h.\"Tenant\"=r.\"Tenant\" AND h.\"EntityId\"=r.\"Id\") OR 1<>(SELECT MIN(h.\"Revision\") FROM \"Revisions\" h WHERE h.\"Tenant\"=r.\"Tenant\" AND h.\"EntityId\"=r.\"Id\") OR r.\"Revision\"<>(SELECT MAX(h.\"Revision\") FROM \"Revisions\" h WHERE h.\"Tenant\"=r.\"Tenant\" AND h.\"EntityId\"=r.\"Id\")",
            "SELECT COUNT(*) FROM \"Revisions\" h LEFT JOIN \"Records\" r ON r.\"Tenant\"=h.\"Tenant\" AND r.\"Id\"=h.\"EntityId\" WHERE r.\"Id\" IS NULL",
            "SELECT COUNT(*) FROM \"Audit\" a LEFT JOIN \"Revisions\" h ON a.\"Tenant\"=h.\"Tenant\" AND a.\"EntityId\"=h.\"EntityId\" AND a.\"Revision\"=h.\"Revision\" WHERE h.\"EntityId\" IS NULL",
            "SELECT COUNT(*) FROM \"Revisions\" h WHERE NOT EXISTS (SELECT 1 FROM \"Audit\" a WHERE a.\"Tenant\"=h.\"Tenant\" AND a.\"EntityId\"=h.\"EntityId\" AND a.\"Revision\"=h.\"Revision\" AND a.\"Action\" IN ('created','revised'))"
        };
        foreach (var sql in checks) if ((await Rows(connection, tx, sql, ct))[0][0] != "0") throw new GraphMigrationException("Legacy evidence history is inconsistent. Restore or repair before adoption.");
    }
}
