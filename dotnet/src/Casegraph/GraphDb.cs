using Microsoft.EntityFrameworkCore;
namespace Casegraph;

public sealed class GraphDb(DbContextOptions<GraphDb> options) : DbContext(options)
{
    internal DbSet<GraphRecord> Records => Set<GraphRecord>();
    internal DbSet<GraphRevision> Revisions => Set<GraphRevision>();
    internal DbSet<AuditRow> Audit => Set<AuditRow>();
    protected override void OnModelCreating(ModelBuilder model)
    {
        model.Entity<GraphRecord>().HasKey(x => new { x.Tenant, x.Id });
        model.Entity<GraphRecord>().Property(x => x.Revision).IsConcurrencyToken();
        model.Entity<GraphRecord>().HasIndex(x => new { x.Tenant, x.Kind });
        model.Entity<GraphRevision>().HasKey(x => new { x.Tenant, x.EntityId, x.Revision });
        model.Entity<GraphRevision>().HasOne<GraphRecord>().WithMany().HasForeignKey(x => new { x.Tenant, x.EntityId }).OnDelete(DeleteBehavior.Restrict);
        model.Entity<AuditRow>().HasKey(x => x.Id);
        model.Entity<AuditRow>().HasIndex(x => new { x.Tenant, x.EntityId });
        model.Entity<AuditRow>().HasOne<GraphRecord>().WithMany().HasForeignKey(x => new { x.Tenant, x.EntityId }).OnDelete(DeleteBehavior.Restrict);
        if (Database.ProviderName == "Npgsql.EntityFrameworkCore.PostgreSQL")
        {
            model.Entity<GraphRecord>().Property(x => x.EnvelopeJson).HasColumnType("jsonb");
            model.Entity<GraphRevision>().Property(x => x.EnvelopeJson).HasColumnType("jsonb");
        }
    }
    void ValidateWrites()
    {
        foreach (var entry in ChangeTracker.Entries())
        {
            if ((entry.Entity is GraphRevision or AuditRow) && entry.State is EntityState.Modified or EntityState.Deleted)
                throw new InvalidOperationException("Evidence history is append-only.");
            if (entry.Entity is GraphRecord record && entry.State is EntityState.Modified or EntityState.Deleted)
            {
                if (entry.State == EntityState.Deleted || record.Kind is "artifact" or "observation" or "relationship" or "workflow.definition")
                    throw new InvalidOperationException("Evidence is immutable.");
                if (entry.Property(nameof(GraphRecord.Kind)).IsModified || entry.Property(nameof(GraphRecord.Sensitivity)).IsModified)
                    throw new InvalidOperationException("Record metadata is immutable.");
            }
        }
    }
    public override int SaveChanges(bool acceptAllChangesOnSuccess) { ValidateWrites(); return base.SaveChanges(acceptAllChangesOnSuccess); }
    public override Task<int> SaveChangesAsync(bool acceptAllChangesOnSuccess, CancellationToken cancellationToken = default)
    { ValidateWrites(); return base.SaveChangesAsync(acceptAllChangesOnSuccess, cancellationToken); }
}
