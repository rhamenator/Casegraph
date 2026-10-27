CREATE FUNCTION public.cg_record_guard() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog,public AS $body$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'protected record' USING ERRCODE='23514'; END IF;
 IF TG_OP='INSERT' THEN
  IF NEW."Revision"<>1 OR NEW."Sensitivity" NOT BETWEEN 0 AND 3 OR jsonb_typeof(NEW."EnvelopeJson")<>'object' THEN RAISE EXCEPTION 'invalid record' USING ERRCODE='23514'; END IF;
 ELSE
  IF OLD."Kind" IN ('artifact','observation','relationship','workflow.definition','receipt')
  OR NEW."Tenant"<>OLD."Tenant" OR NEW."Id"<>OLD."Id" OR NEW."Kind"<>OLD."Kind" OR NEW."Sensitivity"<>OLD."Sensitivity"
  OR NEW."Revision"<>OLD."Revision"+1 OR jsonb_typeof(NEW."EnvelopeJson")<>'object' THEN RAISE EXCEPTION 'protected record' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $body$;
CREATE FUNCTION public.cg_record_snapshot() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
BEGIN
 INSERT INTO public."Revisions" ("Tenant","EntityId","Revision","EnvelopeJson") VALUES (NEW."Tenant",NEW."Id",NEW."Revision",NEW."EnvelopeJson");
 RETURN NEW;
END $body$;
CREATE FUNCTION public.cg_revision_guard() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog,public AS $body$
BEGIN
 IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'immutable history' USING ERRCODE='23514'; END IF;
 IF pg_trigger_depth()<2 OR NOT EXISTS (SELECT 1 FROM public."Records" WHERE "Tenant"=NEW."Tenant" AND "Id"=NEW."EntityId" AND "Revision"=NEW."Revision" AND "EnvelopeJson"=NEW."EnvelopeJson") THEN RAISE EXCEPTION 'invalid revision' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $body$;
CREATE FUNCTION public.cg_audit_guard() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog,public AS $body$
BEGIN
 IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'immutable audit' USING ERRCODE='23514'; END IF;
 IF NEW."Action" NOT IN ('created','revised','exported') OR NOT EXISTS (SELECT 1 FROM public."Revisions" WHERE "Tenant"=NEW."Tenant" AND "EntityId"=NEW."EntityId" AND "Revision"=NEW."Revision") THEN RAISE EXCEPTION 'invalid audit reference' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $body$;
CREATE FUNCTION public.cg_no_truncate() RETURNS trigger LANGUAGE plpgsql SET search_path=pg_catalog,public AS $body$
BEGIN RAISE EXCEPTION 'protected evidence' USING ERRCODE='23514'; END $body$;
CREATE TRIGGER cg_records_guard BEFORE INSERT OR UPDATE OR DELETE ON public."Records" FOR EACH ROW EXECUTE FUNCTION public.cg_record_guard();
CREATE TRIGGER cg_records_snapshot AFTER INSERT OR UPDATE ON public."Records" FOR EACH ROW EXECUTE FUNCTION public.cg_record_snapshot();
CREATE TRIGGER cg_revisions_guard BEFORE INSERT OR UPDATE OR DELETE ON public."Revisions" FOR EACH ROW EXECUTE FUNCTION public.cg_revision_guard();
CREATE TRIGGER cg_audit_guard BEFORE INSERT OR UPDATE OR DELETE ON public."Audit" FOR EACH ROW EXECUTE FUNCTION public.cg_audit_guard();
CREATE TRIGGER cg_records_truncate BEFORE TRUNCATE ON public."Records" FOR EACH STATEMENT EXECUTE FUNCTION public.cg_no_truncate();
CREATE TRIGGER cg_revisions_truncate BEFORE TRUNCATE ON public."Revisions" FOR EACH STATEMENT EXECUTE FUNCTION public.cg_no_truncate();
CREATE TRIGGER cg_audit_truncate BEFORE TRUNCATE ON public."Audit" FOR EACH STATEMENT EXECUTE FUNCTION public.cg_no_truncate();
REVOKE ALL ON FUNCTION public.cg_record_guard(),public.cg_record_snapshot(),public.cg_revision_guard(),public.cg_audit_guard(),public.cg_no_truncate() FROM PUBLIC;
