CREATE TRIGGER claims_no_update
BEFORE UPDATE ON claims
BEGIN
    SELECT RAISE(ABORT, 'claims are immutable; append state, review, or correction');
END;

CREATE TRIGGER claims_no_delete
BEFORE DELETE ON claims
BEGIN
    SELECT RAISE(ABORT, 'claims are immutable');
END;

CREATE TRIGGER evidence_no_update
BEFORE UPDATE ON evidence
BEGIN
    SELECT RAISE(ABORT, 'evidence is immutable');
END;

CREATE TRIGGER evidence_no_delete
BEFORE DELETE ON evidence
BEGIN
    SELECT RAISE(ABORT, 'evidence is immutable');
END;

CREATE TRIGGER human_reviews_no_update
BEFORE UPDATE ON human_reviews
BEGIN
    SELECT RAISE(ABORT, 'human reviews are append-only');
END;

CREATE TRIGGER human_reviews_no_delete
BEFORE DELETE ON human_reviews
BEGIN
    SELECT RAISE(ABORT, 'human reviews are append-only');
END;

CREATE TRIGGER rule_versions_no_update
BEFORE UPDATE ON rule_versions
BEGIN
    SELECT RAISE(ABORT, 'rule versions are immutable');
END;

CREATE TRIGGER rule_versions_no_delete
BEFORE DELETE ON rule_versions
BEGIN
    SELECT RAISE(ABORT, 'rule versions are immutable');
END;

CREATE TRIGGER rule_evaluations_no_update
BEFORE UPDATE ON rule_evaluations
BEGIN
    SELECT RAISE(ABORT, 'rule evaluations are immutable');
END;

CREATE TRIGGER rule_evaluations_no_delete
BEFORE DELETE ON rule_evaluations
BEGIN
    SELECT RAISE(ABORT, 'rule evaluations are immutable');
END;

