use munin::{
    Card, Error, Kind, NewEntry, Options, Owner, Scope, SearchQuery, Status, Store, TagUse,
};

const DAY: i64 = 86_400_000;
const NOW: i64 = 1_800_000_000_000;

fn add(store: &mut Store, project: &str, kind: Kind, summary: &str, age_days: i64) -> i64 {
    add_entry(
        store,
        NewEntry {
            kind,
            ..entry(project, summary)
        },
        age_days,
    )
}

fn add_entry(store: &mut Store, new: NewEntry, age_days: i64) -> i64 {
    store.append(&new, NOW - age_days * DAY).unwrap()
}

fn entry(project: &str, summary: &str) -> NewEntry {
    NewEntry {
        project: project.to_owned(),
        group: None,
        kind: Kind::Note,
        summary: summary.to_owned(),
        body: String::new(),
        tags: Vec::new(),
        supersedes: None,
    }
}

fn query(project: &str, text: &str) -> SearchQuery {
    SearchQuery {
        text: text.to_owned(),
        project: project.to_owned(),
        scope: None,
        include_inactive: false,
        deep_history: false,
        limit: None,
    }
}

fn ids(cards: &[Card]) -> Vec<i64> {
    cards.iter().map(|c| c.id).collect()
}

fn sorted(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort();
    ids
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

// Check that words are OR-ed and matched case-insensitively.
#[test]
fn words_are_ored() {
    let mut store = Store::open_in_memory().unwrap();
    let wal = add(&mut store, "scry", Kind::Note, "Use WAL mode", 0);
    let tokio = add(&mut store, "scry", Kind::Note, "tokio runtime", 0);
    add(&mut store, "scry", Kind::Note, "unrelated", 0);

    let hits = store.search(&query("scry", "wal TOKIO"), NOW).unwrap();
    assert_eq!(sorted(ids(&hits)), vec![wal, tokio]);
}

#[test]
fn quoted_phrase_matches_in_order() {
    let mut store = Store::open_in_memory().unwrap();
    let phrase = add(&mut store, "scry", Kind::Note, "set busy timeout", 0);
    add(&mut store, "scry", Kind::Note, "timeout when busy", 0);

    let hits = store
        .search(&query("scry", "\"busy timeout\""), NOW)
        .unwrap();
    assert_eq!(ids(&hits), vec![phrase]);
}

// Check that FTS5 syntax in user text cannot break the query.
#[test]
fn operators_in_text_are_harmless() {
    let mut store = Store::open_in_memory().unwrap();
    add(&mut store, "scry", Kind::Note, "near and not", 0);
    for text in ["NEAR(", "AND", "a OR", "\"open", "x* ^y", "col:term", "(a"] {
        store
            .search(&query("scry", text), NOW)
            .unwrap_or_else(|e| panic!("{text:?}: {e}"));
    }
}

#[test]
fn query_without_words_is_rejected() {
    let store = Store::open_in_memory().unwrap();
    assert!(matches!(
        store.search(&query("scry", " -- ... "), NOW),
        Err(Error::Invalid { field: "query", .. })
    ));
}

#[test]
fn decaying_kinds_lose_score_with_age() {
    let mut store = Store::open_in_memory().unwrap();
    let old = add(&mut store, "scry", Kind::Bugfix, "flaky socket test", 60);
    let new = add(&mut store, "scry", Kind::Bugfix, "flaky socket test", 0);

    let hits = store.search(&query("scry", "socket"), NOW).unwrap();
    assert_eq!(ids(&hits), vec![new, old]);
    let ratio = hits[1].score / hits[0].score;
    assert!((ratio - 0.25).abs() < 1e-9, "ratio {ratio}");
}

#[test]
fn decisions_do_not_decay() {
    let mut store = Store::open_in_memory().unwrap();
    let decision = add(
        &mut store,
        "scry",
        Kind::Decision,
        "socket layer choice",
        365,
    );
    let note = add(&mut store, "scry", Kind::Note, "socket layer choice", 365);

    let hits = store.search(&query("scry", "socket"), NOW).unwrap();
    assert_eq!(ids(&hits), vec![decision, note]);
    assert!(hits[1].score < hits[0].score * 1e-3);
}

#[test]
fn deep_history_ignores_age() {
    let mut store = Store::open_in_memory().unwrap();
    add(&mut store, "scry", Kind::Bugfix, "flaky socket test", 60);
    add(&mut store, "scry", Kind::Bugfix, "flaky socket test", 0);

    let hits = store
        .search(
            &SearchQuery {
                deep_history: true,
                ..query("scry", "socket")
            },
            NOW,
        )
        .unwrap();
    assert!((hits[0].score - hits[1].score).abs() < 1e-9);
}

// Check that decay is applied before the limit: an immune decision with weaker
// text relevance beats many strong but long-decayed bugfixes.
#[test]
fn ranking_covers_all_matches_before_limit() {
    let mut store = Store::open_in_memory().unwrap();
    for i in 0..35 {
        add(
            &mut store,
            "scry",
            Kind::Bugfix,
            &format!("cache cache cache fix {i}"),
            365,
        );
    }
    let decision = add_entry(
        &mut store,
        NewEntry {
            kind: Kind::Decision,
            body: "a long body that mentions the cache only once among many other words".into(),
            ..entry("scry", "storage layout")
        },
        365,
    );

    let raw = store
        .search(
            &SearchQuery {
                deep_history: true,
                limit: Some(1),
                ..query("scry", "cache")
            },
            NOW,
        )
        .unwrap();
    assert_ne!(
        ids(&raw),
        vec![decision],
        "decision should be weaker on text alone"
    );

    let hits = store
        .search(
            &SearchQuery {
                limit: Some(1),
                ..query("scry", "cache")
            },
            NOW,
        )
        .unwrap();
    assert_eq!(ids(&hits), vec![decision]);
}

#[test]
fn inactive_entries_are_hidden_unless_asked() {
    let mut store = Store::open_in_memory().unwrap();
    let old = add(&mut store, "scry", Kind::Decision, "retry policy v1", 2);
    let new = add_entry(
        &mut store,
        NewEntry {
            supersedes: Some(old),
            ..entry("scry", "retry policy v2")
        },
        1,
    );
    let wrong = add(&mut store, "scry", Kind::Note, "retry policy myth", 0);
    store.retract(wrong, "not true", NOW).unwrap();

    let hits = store.search(&query("scry", "retry"), NOW).unwrap();
    assert_eq!(ids(&hits), vec![new]);

    let all = store
        .search(
            &SearchQuery {
                include_inactive: true,
                ..query("scry", "retry")
            },
            NOW,
        )
        .unwrap();
    assert_eq!(sorted(ids(&all)), vec![old, new, wrong]);
    let old_card = all.iter().find(|c| c.id == old).unwrap();
    assert_eq!(old_card.status, Status::Superseded);
    assert_eq!(old_card.superseded_by, Some(new));
    let wrong_card = all.iter().find(|c| c.id == wrong).unwrap();
    assert_eq!(wrong_card.status, Status::Retracted);
}

struct Fleet {
    store: Store,
    billing: i64,
    auth: i64,
    platform: i64,
    scry: i64,
}

// billing and auth form the group "platform"; scry is in no group.
fn fleet() -> Fleet {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_members("platform", &names(&["billing", "auth"]))
        .unwrap();
    let billing = add(&mut store, "billing", Kind::Note, "event schema", 0);
    let auth = add(&mut store, "auth", Kind::Note, "event schema", 0);
    let platform = add_entry(
        &mut store,
        NewEntry {
            group: Some("platform".to_owned()),
            kind: Kind::Decision,
            ..entry("billing", "event schema")
        },
        0,
    );
    let scry = add(&mut store, "scry", Kind::Note, "event schema", 0);
    Fleet {
        store,
        billing,
        auth,
        platform,
        scry,
    }
}

fn scoped(fleet: &Fleet, project: &str, scope: Option<Scope>) -> Result<Vec<i64>, Error> {
    let hits = fleet.store.search(
        &SearchQuery {
            scope,
            ..query(project, "event")
        },
        NOW,
    )?;
    Ok(sorted(ids(&hits)))
}

#[test]
fn default_scope_is_group_for_members() {
    let f = fleet();
    assert_eq!(
        scoped(&f, "billing", None).unwrap(),
        sorted(vec![f.billing, f.auth, f.platform])
    );
    assert_eq!(scoped(&f, "scry", None).unwrap(), vec![f.scry]);
}

#[test]
fn explicit_scopes() {
    let f = fleet();
    assert_eq!(
        scoped(&f, "billing", Some(Scope::Project)).unwrap(),
        vec![f.billing]
    );
    assert_eq!(
        scoped(&f, "scry", Some(Scope::Group(Some("platform".into())))).unwrap(),
        sorted(vec![f.billing, f.auth, f.platform])
    );
    assert_eq!(
        scoped(&f, "scry", Some(Scope::All)).unwrap(),
        sorted(vec![f.billing, f.auth, f.platform, f.scry])
    );
    assert!(matches!(
        scoped(&f, "scry", Some(Scope::Group(Some("nope".into())))),
        Err(Error::GroupNotFound(_))
    ));
}

#[test]
fn cards_name_their_owner() {
    let f = fleet();
    let hits = f.store.search(&query("auth", "event"), NOW).unwrap();
    let platform = hits.iter().find(|c| c.id == f.platform).unwrap();
    assert_eq!(platform.owner, Owner::Group("platform".to_owned()));
    let billing = hits.iter().find(|c| c.id == f.billing).unwrap();
    assert_eq!(billing.owner, Owner::Project("billing".to_owned()));
}

#[test]
fn limit_default_and_bounds() {
    let mut store = Store::open_in_memory().unwrap();
    for i in 0..7 {
        add(
            &mut store,
            "scry",
            Kind::Note,
            &format!("cache note {i}"),
            0,
        );
    }
    assert_eq!(store.search(&query("scry", "cache"), NOW).unwrap().len(), 5);
    for bad in [0, 16] {
        let res = store.search(
            &SearchQuery {
                limit: Some(bad),
                ..query("scry", "cache")
            },
            NOW,
        );
        assert!(
            matches!(res, Err(Error::Invalid { field: "limit", .. })),
            "{bad}"
        );
    }
}

#[test]
fn card_carries_tags_and_age() {
    let mut store = Store::open_in_memory().unwrap();
    let id = add_entry(
        &mut store,
        NewEntry {
            tags: names(&["sqlite", "wal"]),
            ..entry("scry", "checkpoint tuning")
        },
        3,
    );
    let hits = store.search(&query("scry", "checkpoint"), NOW).unwrap();
    assert_eq!(hits[0].id, id);
    assert_eq!(hits[0].tags, names(&["sqlite", "wal"]));
    assert_eq!(hits[0].age_days, 3);
    assert_eq!(hits[0].kind, Kind::Note);
}

// Check that tags rank by recent use, then total use, and skip inactive
// entries and other scopes.
#[test]
fn tags_by_recent_use() {
    let mut store = Store::open_in_memory().unwrap();
    let tagged = |tags: &[&str]| NewEntry {
        tags: names(tags),
        ..entry("scry", "x")
    };
    add_entry(&mut store, tagged(&["sqlite", "wal"]), 0);
    add_entry(&mut store, tagged(&["sqlite", "old"]), 90);
    add_entry(&mut store, tagged(&["old"]), 90);
    let wrong = add_entry(&mut store, tagged(&["gone"]), 0);
    store.retract(wrong, "wrong", NOW).unwrap();
    add_entry(
        &mut store,
        NewEntry {
            tags: names(&["foreign"]),
            ..entry("billing", "x")
        },
        0,
    );

    let tags = store.tags("scry", None, None, NOW).unwrap();
    let expected = [("sqlite", 1, 2), ("wal", 1, 1), ("old", 0, 2)]
        .map(|(tag, recent, total)| TagUse {
            tag: tag.to_owned(),
            recent,
            total,
        })
        .to_vec();
    assert_eq!(tags, expected);

    assert_eq!(store.tags("scry", None, Some(1), NOW).unwrap().len(), 1);
    assert!(matches!(
        store.tags("scry", None, Some(0), NOW),
        Err(Error::Invalid { field: "limit", .. })
    ));
}

#[test]
fn half_life_is_configurable() {
    let mut store = Store::open_in_memory_with(Options {
        half_life_days: 1.0,
    })
    .unwrap();
    add(&mut store, "scry", Kind::Note, "socket", 1);
    add(&mut store, "scry", Kind::Note, "socket", 0);
    let hits = store.search(&query("scry", "socket"), NOW).unwrap();
    let ratio = hits[1].score / hits[0].score;
    assert!((ratio - 0.5).abs() < 1e-9, "ratio {ratio}");

    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            Store::open_in_memory_with(Options {
                half_life_days: bad
            }),
            Err(Error::Invalid {
                field: "half_life_days",
                ..
            })
        ));
    }
}
