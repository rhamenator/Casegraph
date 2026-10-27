using System.Diagnostics;
using System.Security.Cryptography;
using System.Text.Json;
using Casegraph;
using Microsoft.EntityFrameworkCore;

var directory = Path.Combine(Path.GetTempPath(), $"casegraph-synthetic-benchmark-{Guid.NewGuid():N}");
Directory.CreateDirectory(directory);
try
{
    var path = Path.Combine(directory, "benchmark.db");
    await using (var db = new GraphDb(new DbContextOptionsBuilder<GraphDb>().UseSqlite($"Data Source={path};Pooling=False").Options))
    {
        await GraphMigrator.ApplyAsync(db);
        var graph = new GraphStore(db, new PayloadCipher(new KeyRing("synthetic", new Dictionary<string, byte[]> { ["synthetic"] = RandomNumberGenerator.GetBytes(32) })), new("benchmark", "synthetic-runner"));
        var ids = new List<Guid>(); var watch = Stopwatch.StartNew();
        for (var i = 0; i < 100; i++) ids.Add((await graph.CreateAsync("benchmark.item", new { Index = i, Text = "Synthetic benchmark data" })).Id);
        var insertMs = watch.Elapsed.TotalMilliseconds; watch.Restart();
        foreach (var id in ids.Take(50)) await graph.ReviseAsync(id, 1, new { Text = "Synthetic correction" });
        var reviseMs = watch.Elapsed.TotalMilliseconds; watch.Restart();
        var page = await graph.ListPageAsync("benchmark.item", 0, 100);
        var readMs = watch.Elapsed.TotalMilliseconds;
        if (page.Count != 100 || (await graph.HistoryAsync(ids[0])).Count != 2) throw new InvalidOperationException("Benchmark integrity failure.");
        Console.WriteLine(JsonSerializer.Serialize(new { Workload = "SQLite file, AES-GCM encrypted generic nodes, single process", Inserts = 100, Revisions = 50, PageReads = 100, InsertMs = Math.Round(insertMs, 2), ReviseMs = Math.Round(reviseMs, 2), ReadMs = Math.Round(readMs, 2), Runtime = Environment.Version.ToString() }));
    }
    Console.WriteLine(JsonSerializer.Serialize(new { DatabaseBytes = new FileInfo(path).Length }));
}
finally { Directory.Delete(directory, true); }
