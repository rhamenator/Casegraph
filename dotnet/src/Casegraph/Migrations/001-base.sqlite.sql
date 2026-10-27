CREATE TABLE "Records" (
 "Tenant" TEXT NOT NULL, "Id" TEXT NOT NULL, "Kind" TEXT NOT NULL, "Revision" INTEGER NOT NULL,
 "Sensitivity" INTEGER NOT NULL, "EnvelopeJson" TEXT NOT NULL,
 CONSTRAINT "PK_Records" PRIMARY KEY ("Tenant", "Id"));
CREATE TABLE "Revisions" (
 "Tenant" TEXT NOT NULL, "EntityId" TEXT NOT NULL, "Revision" INTEGER NOT NULL, "EnvelopeJson" TEXT NOT NULL,
 CONSTRAINT "PK_Revisions" PRIMARY KEY ("Tenant", "EntityId", "Revision"),
 CONSTRAINT "FK_Revisions_Records_Tenant_EntityId" FOREIGN KEY ("Tenant", "EntityId") REFERENCES "Records" ("Tenant", "Id") ON DELETE RESTRICT);
CREATE TABLE "Audit" (
 "Id" TEXT NOT NULL, "Tenant" TEXT NOT NULL, "Actor" TEXT NOT NULL, "EntityId" TEXT NOT NULL,
 "Revision" INTEGER NOT NULL, "Action" TEXT NOT NULL, "At" TEXT NOT NULL,
 CONSTRAINT "PK_Audit" PRIMARY KEY ("Id"),
 CONSTRAINT "FK_Audit_Records_Tenant_EntityId" FOREIGN KEY ("Tenant", "EntityId") REFERENCES "Records" ("Tenant", "Id") ON DELETE RESTRICT);
CREATE INDEX "IX_Records_Tenant_Kind" ON "Records" ("Tenant", "Kind");
CREATE INDEX "IX_Audit_Tenant_EntityId" ON "Audit" ("Tenant", "EntityId");
