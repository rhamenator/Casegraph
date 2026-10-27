CREATE TRIGGER cg_records_insert BEFORE INSERT ON "Records" BEGIN
 SELECT CASE WHEN EXISTS (SELECT 1 FROM "Records" WHERE "Tenant"=NEW."Tenant" AND "Id"=NEW."Id") THEN RAISE(ABORT,'casegraph identity conflict') END;
 SELECT CASE WHEN NEW."Revision" != 1 OR NEW."Sensitivity" NOT BETWEEN 0 AND 3 OR NOT json_valid(NEW."EnvelopeJson") THEN RAISE(ABORT,'invalid record') END;
END;
CREATE TRIGGER cg_records_update BEFORE UPDATE ON "Records" BEGIN
 SELECT CASE WHEN OLD."Kind" IN ('artifact','observation','relationship','workflow.definition','receipt')
 OR NEW."Tenant" != OLD."Tenant" OR NEW."Id" != OLD."Id" OR NEW."Kind" != OLD."Kind" OR NEW."Sensitivity" != OLD."Sensitivity"
 OR NEW."Revision" != OLD."Revision" + 1 OR NOT json_valid(NEW."EnvelopeJson") THEN RAISE(ABORT,'protected record') END;
END;
CREATE TRIGGER cg_records_delete BEFORE DELETE ON "Records" BEGIN SELECT RAISE(ABORT,'protected record'); END;
CREATE TRIGGER cg_records_history_insert AFTER INSERT ON "Records" BEGIN
 INSERT INTO "Revisions" ("Tenant","EntityId","Revision","EnvelopeJson") VALUES (NEW."Tenant",NEW."Id",NEW."Revision",NEW."EnvelopeJson");
END;
CREATE TRIGGER cg_records_history_update AFTER UPDATE ON "Records" BEGIN
 INSERT INTO "Revisions" ("Tenant","EntityId","Revision","EnvelopeJson") VALUES (NEW."Tenant",NEW."Id",NEW."Revision",NEW."EnvelopeJson");
END;
CREATE TRIGGER cg_revisions_insert BEFORE INSERT ON "Revisions" BEGIN
 SELECT CASE WHEN EXISTS (SELECT 1 FROM "Revisions" WHERE "Tenant"=NEW."Tenant" AND "EntityId"=NEW."EntityId" AND "Revision"=NEW."Revision") OR NOT EXISTS (SELECT 1 FROM "Records" WHERE "Tenant"=NEW."Tenant" AND "Id"=NEW."EntityId" AND "Revision"=NEW."Revision" AND "EnvelopeJson"=NEW."EnvelopeJson") THEN RAISE(ABORT,'invalid revision') END;
END;
CREATE TRIGGER cg_revisions_update BEFORE UPDATE ON "Revisions" BEGIN SELECT RAISE(ABORT,'immutable history'); END;
CREATE TRIGGER cg_revisions_delete BEFORE DELETE ON "Revisions" BEGIN SELECT RAISE(ABORT,'immutable history'); END;
CREATE TRIGGER cg_audit_insert BEFORE INSERT ON "Audit" BEGIN
 SELECT CASE WHEN EXISTS (SELECT 1 FROM "Audit" WHERE "Id"=NEW."Id") OR NEW."Action" NOT IN ('created','revised','exported') OR NOT EXISTS
 (SELECT 1 FROM "Revisions" WHERE "Tenant"=NEW."Tenant" AND "EntityId"=NEW."EntityId" AND "Revision"=NEW."Revision") THEN RAISE(ABORT,'invalid audit reference') END;
END;
CREATE TRIGGER cg_audit_update BEFORE UPDATE ON "Audit" BEGIN SELECT RAISE(ABORT,'immutable audit'); END;
CREATE TRIGGER cg_audit_delete BEFORE DELETE ON "Audit" BEGIN SELECT RAISE(ABORT,'immutable audit'); END;
