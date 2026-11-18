CREATE TABLE node_session_automatic_releases (
  operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=22),
  anchor_reservation TEXT NOT NULL
    REFERENCES node_session_capacity(reservation_id),
  reason TEXT NOT NULL
    CHECK(reason IN ('idle_timeout','capacity_pressure')),
  phase TEXT NOT NULL
    CHECK(phase IN ('pending','finished','withdrawn')),
  outcome TEXT
    CHECK(outcome IN ('requested','failed','unknown','withdrawn')),
  CHECK((phase='withdrawn')=(outcome IS 'withdrawn'))
) STRICT;
CREATE TABLE node_session_automatic_release_members (
  operation_id TEXT NOT NULL
    REFERENCES node_session_automatic_releases(operation_id),
  reservation_id TEXT NOT NULL
    REFERENCES node_session_capacity(reservation_id),
  PRIMARY KEY(operation_id,reservation_id)
) STRICT;
CREATE INDEX node_session_automatic_release_member_lookup
  ON node_session_automatic_release_members(reservation_id,operation_id);
INSERT INTO node_state VALUES(1,0);
INSERT INTO product_control VALUES(1,'AAAAAAAAAAAAAAAAAAAAAA',0);
PRAGMA user_version=13;
