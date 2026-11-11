//! Tests for the smart-case rule engine (rules.rs). Synthetic tracks only.

use std::collections::{HashMap, HashSet};

use super::rules::*;
use pm_formats::library::{Track, TrackExtra};

const NOW: i64 = 1_000 * MS_PER_DAY;

fn track(id: i64) -> Track {
    Track {
        track_id: id,
        title: format!("T{id}"),
        artist: "A".into(),
        album: String::new(),
        genre: "House".into(),
        bpm: 128.0,
        key: "8A".into(),
        play_time_secs: 200,
        rating: 3,
        location: String::new(),
        file_size: 1000,
        format: "MP3".into(),
        analysed: true,
        times_played: 0,
        last_played: -1,
    }
}

fn extra() -> TrackExtra {
    TrackExtra { year: 2001, date_added: NOW - 10 * MS_PER_DAY, comments: "synthetic comment".into(), label: "Synthetic Label".into(), remixer: "Synthetic Remixer".into() }
}

fn data(tracks: Vec<Track>, extras: Vec<TrackExtra>) -> Data {
    let extras = tracks.iter().map(|t| t.track_id).zip(extras).collect();
    Data { tracks, extras, favourites: HashSet::new(), cases: HashMap::new(), now_ms: NOW }
}

fn one(rule: &Rule, t: &Track, x: &TrackExtra) -> bool {
    let favs = HashSet::from([1]);
    let cases = HashMap::from([(7, Some(HashSet::from([t.track_id]))), (8, Some(HashSet::new()))]);
    rule_matches(rule, t, x, &Context { favourites: &favs, cases: &cases, now_ms: NOW })
}

fn q(rules: Vec<Rule>) -> Query {
    Query { rules, ..Default::default() }
}

fn ids(query: &Query, d: &Data) -> Vec<i64> {
    evaluate(query, d).unwrap().ids
}

fn text(field: TextField, op: TextOp, s: &str) -> Rule {
    Rule::Text { field, op, text: s.into() }
}

#[test]
fn camelot_parsing_covers_every_notation() {
    let c = |n, minor| Some(Camelot { number: n, minor });
    assert_eq!(Camelot::parse("8A"), c(8, true));
    assert_eq!(Camelot::parse(" 08b "), c(8, false));
    assert_eq!(Camelot::parse("12a"), c(12, true));
    assert_eq!(Camelot::parse("13A"), None);
    assert_eq!(Camelot::parse("0A"), None);
    assert_eq!(Camelot::parse("8C"), None);
    assert_eq!(Camelot::parse("Am"), c(8, true));
    assert_eq!(Camelot::parse("A minor"), c(8, true));
    assert_eq!(Camelot::parse("C"), c(8, false));
    assert_eq!(Camelot::parse("C major"), c(8, false));
    assert_eq!(Camelot::parse("Cmaj"), c(8, false));
    assert_eq!(Camelot::parse("F#m"), c(11, true));
    assert_eq!(Camelot::parse("Gbm"), c(11, true));
    assert_eq!(Camelot::parse("Ab Minor"), c(1, true));
    assert_eq!(Camelot::parse("G#min"), c(1, true));
    assert_eq!(Camelot::parse("Bb"), c(6, false));
    assert_eq!(Camelot::parse("B"), c(1, false));
    assert_eq!(Camelot::parse("Bm"), c(10, true));
    assert_eq!(Camelot::parse("Cb"), c(1, false)); // Cb = B
    assert_eq!(Camelot::parse("E#m"), c(4, true)); // E# = F
    assert_eq!(Camelot::parse("A\u{266d}m"), c(1, true));
    assert_eq!(Camelot::parse(""), None);
    assert_eq!(Camelot::parse("H"), None);
    assert_eq!(Camelot::parse("Amajor7"), None);
    // relative keys share a number
    assert_eq!(Camelot::parse("Am").unwrap().number, Camelot::parse("C").unwrap().number);
    assert_eq!(Camelot::parse("F#m").unwrap().number, Camelot::parse("A").unwrap().number);
}

#[test]
fn camelot_compatibility_wraps_the_wheel() {
    let k = |s: &str| Camelot::parse(s).unwrap();
    assert!(k("8A").compatible(k("8A")));
    assert!(k("8A").compatible(k("9A")));
    assert!(k("8A").compatible(k("7A")));
    assert!(k("8A").compatible(k("8B")));
    assert!(!k("8A").compatible(k("9B")));
    assert!(!k("8A").compatible(k("10A")));
    assert!(k("12A").compatible(k("1A")), "wraps 12 -> 1");
    assert!(k("1B").compatible(k("12B")));
    assert!(!k("12A").compatible(k("2A")));
    assert!(k("Am").compatible(k("Dm")), "7A is next to 8A");
}

#[test]
fn the_original_rules_still_match_what_they_say() {
    let mut t = track(1);
    let x = extra();
    assert!(one(&Rule::RatingMin { stars: 3 }, &t, &x));
    assert!(!one(&Rule::RatingMin { stars: 4 }, &t, &x));
    assert!(one(&Rule::Favourite { value: true }, &t, &x));
    assert!(!one(&Rule::Favourite { value: false }, &t, &x));
    assert!(one(&Rule::Favourite { value: false }, &track(2), &x), "track 2 is not a favourite");

    assert!(one(&Rule::PlaysZero, &t, &x));
    assert!(!one(&Rule::PlaysMin { n: 1 }, &t, &x));
    t.times_played = 5;
    assert!(!one(&Rule::PlaysZero, &t, &x));
    assert!(one(&Rule::PlaysMin { n: 5 }, &t, &x));
    assert!(!one(&Rule::PlaysMin { n: 6 }, &t, &x));

    // last played: never (-1) and 0 never match; the window is inclusive
    assert!(!one(&Rule::PlayedWithin { days: 30 }, &t, &x));
    t.last_played = 0;
    assert!(!one(&Rule::PlayedWithin { days: 30 }, &t, &x));
    t.last_played = NOW - 10 * MS_PER_DAY;
    assert!(one(&Rule::PlayedWithin { days: 10 }, &t, &x));
    assert!(!one(&Rule::PlayedWithin { days: 9 }, &t, &x));

    assert!(one(&Rule::BpmRange { min: Some(128.0), max: Some(128.0) }, &t, &x));
    assert!(one(&Rule::BpmRange { min: Some(120.0), max: None }, &t, &x));
    assert!(one(&Rule::BpmRange { min: None, max: Some(130.0) }, &t, &x));
    assert!(!one(&Rule::BpmRange { min: Some(128.5), max: None }, &t, &x));
    assert!(!one(&Rule::BpmRange { min: None, max: Some(127.9) }, &t, &x));
    t.bpm = 0.0;
    assert!(!one(&Rule::BpmRange { min: None, max: Some(200.0) }, &t, &x), "no BPM never matches");

    t.key = "Am".into();
    assert!(one(&Rule::Key { key: "8A".into() }, &t, &x), "notation does not matter");
    assert!(!one(&Rule::Key { key: "9A".into() }, &t, &x));
    assert!(one(&Rule::KeyCompatible { key: "9A".into() }, &t, &x));
    assert!(one(&Rule::KeyCompatible { key: "C".into() }, &t, &x));
    assert!(!one(&Rule::KeyCompatible { key: "3B".into() }, &t, &x));
    t.key = String::new();
    assert!(!one(&Rule::Key { key: "8A".into() }, &t, &x), "unknown key never matches");
    assert!(!one(&Rule::KeyCompatible { key: "8A".into() }, &t, &x));
    assert!(!one(&Rule::Key { key: "nonsense".into() }, &track(2), &x), "invalid rule key matches nothing");

    assert!(one(&Rule::Genre { text: " HOU ".into() }, &t, &x), "the old genre rule is still understood");
    assert!(!one(&Rule::Genre { text: "techno".into() }, &t, &x));

    assert!(one(&Rule::Analysed { value: true }, &t, &x));
    assert!(!one(&Rule::Analysed { value: false }, &t, &x));
    t.analysed = false;
    assert!(one(&Rule::Analysed { value: false }, &t, &x));
}

#[test]
fn text_rules_contain_equal_and_start_ignoring_case_and_blanks() {
    let mut t = track(1);
    t.title = "Title One".into();
    t.artist = "Artist One".into();
    let x = extra();
    for (field, have) in [
        (TextField::Title, "Title One"),
        (TextField::Artist, "Artist One"),
        (TextField::Genre, "House"),
        (TextField::Label, "Synthetic Label"),
        (TextField::Comments, "synthetic comment"),
        (TextField::Remixer, "Synthetic Remixer"),
    ] {
        let up = have.to_uppercase();
        assert!(one(&text(field, TextOp::Is, &format!("  {up} ")), &t, &x), "{field:?} is");
        assert!(one(&text(field, TextOp::Contains, &have[1..]), &t, &x), "{field:?} contains");
        assert!(one(&text(field, TextOp::StartsWith, &have[..1].to_lowercase()), &t, &x), "{field:?} starts with");
        assert!(!one(&text(field, TextOp::Is, &have[1..]), &t, &x), "{field:?} is is not contains");
        assert!(!one(&text(field, TextOp::StartsWith, &have[1..]), &t, &x), "{field:?} starts with is not contains");
    }
    assert!(!one(&text(TextField::Album, TextOp::Contains, "x"), &t, &x), "an empty column contains nothing");
    assert!(one(&text(TextField::Label, TextOp::Contains, "label"), &t, &x));
    // a unicode letter case-folds
    let mut u = track(2);
    u.title = "\u{00c9}t\u{00e9}".into();
    assert!(one(&text(TextField::Title, TextOp::Is, "\u{00e9}t\u{00c9}"), &u, &x));
}

#[test]
fn year_date_duration_plays_format_and_stale_rules() {
    let mut t = track(1);
    let mut x = extra();

    assert!(one(&Rule::YearRange { min: Some(2001), max: Some(2001) }, &t, &x));
    assert!(one(&Rule::YearRange { min: Some(1990), max: None }, &t, &x));
    assert!(!one(&Rule::YearRange { min: None, max: Some(2000) }, &t, &x));
    x.year = 0;
    assert!(!one(&Rule::YearRange { min: None, max: Some(2000) }, &t, &x), "no year never matches");
    x.year = 2001;

    // added 10 days ago: within is inclusive, before is strict
    assert!(one(&Rule::AddedWithin { days: 10 }, &t, &x));
    assert!(!one(&Rule::AddedWithin { days: 9 }, &t, &x));
    assert!(one(&Rule::AddedBefore { days: 9 }, &t, &x));
    assert!(!one(&Rule::AddedBefore { days: 10 }, &t, &x));
    x.date_added = 0;
    assert!(!one(&Rule::AddedWithin { days: 9999 }, &t, &x), "unknown date never matches");
    assert!(!one(&Rule::AddedBefore { days: 1 }, &t, &x));
    x.date_added = NOW - 10 * MS_PER_DAY;

    assert!(one(&Rule::DurationRange { min_secs: Some(200), max_secs: Some(200) }, &t, &x));
    assert!(!one(&Rule::DurationRange { min_secs: Some(201), max_secs: None }, &t, &x));
    assert!(!one(&Rule::DurationRange { min_secs: None, max_secs: Some(199) }, &t, &x));
    t.play_time_secs = 0;
    assert!(!one(&Rule::DurationRange { min_secs: None, max_secs: Some(500) }, &t, &x), "unknown length never matches");
    t.play_time_secs = 200;

    t.times_played = 2;
    assert!(one(&Rule::PlaysRange { min: Some(2), max: Some(2) }, &t, &x));
    assert!(!one(&Rule::PlaysRange { min: Some(3), max: None }, &t, &x));
    assert!(one(&Rule::PlaysRange { min: None, max: Some(2) }, &t, &x));
    t.times_played = 0;
    assert!(one(&Rule::PlaysRange { min: Some(0), max: Some(1) }, &t, &x), "0 is a valid bound");

    assert!(one(&Rule::Format { value: " mp3 ".into() }, &t, &x));
    assert!(!one(&Rule::Format { value: "FLAC".into() }, &t, &x));

    // stale: played more than N days ago; never played follows include_never
    let stale = |days, include_never| Rule::LastPlayedBefore { days, include_never };
    assert!(one(&stale(30, true), &t, &x), "never played counts by default");
    assert!(!one(&stale(30, false), &t, &x));
    t.last_played = NOW - 40 * MS_PER_DAY;
    assert!(one(&stale(30, false), &t, &x));
    assert!(one(&stale(39, false), &t, &x));
    assert!(!one(&stale(40, false), &t, &x), "exactly N days ago is not older than N days");
    assert!(!one(&stale(41, true), &t, &x));
    t.last_played = 0;
    assert!(one(&stale(30, true), &t, &x), "0 is the never-played sentinel too");

    // case membership
    assert!(one(&Rule::InCase { case_id: 7, value: true }, &t, &x));
    assert!(!one(&Rule::InCase { case_id: 7, value: false }, &t, &x));
    assert!(!one(&Rule::InCase { case_id: 8, value: true }, &t, &x));
    assert!(one(&Rule::InCase { case_id: 8, value: false }, &t, &x));
}

#[test]
fn groups_combine_all_any_and_none_and_nest() {
    let t = track(1); // rating 3, 128 bpm, House
    let x = extra();
    let yes = || Rule::RatingMin { stars: 3 };
    let no = || Rule::RatingMin { stars: 5 };
    let grp = |mode, items| Rule::Group { mode, items };

    assert!(one(&grp(Mode::All, vec![yes(), yes()]), &t, &x));
    assert!(!one(&grp(Mode::All, vec![yes(), no()]), &t, &x));
    assert!(one(&grp(Mode::Any, vec![no(), yes()]), &t, &x));
    assert!(!one(&grp(Mode::Any, vec![no(), no()]), &t, &x));
    assert!(one(&grp(Mode::None, vec![no(), no()]), &t, &x));
    assert!(!one(&grp(Mode::None, vec![no(), yes()]), &t, &x));

    // (rating 5 OR (House AND 120-130 BPM)) AND NOT (key 9A or label starts with "z")
    let tree = grp(
        Mode::All,
        vec![
            grp(Mode::Any, vec![no(), grp(Mode::All, vec![text(TextField::Genre, TextOp::Is, "house"), Rule::BpmRange { min: Some(120.0), max: Some(130.0) }])]),
            grp(Mode::None, vec![Rule::Key { key: "9A".into() }, text(TextField::Label, TextOp::StartsWith, "z")]),
        ],
    );
    assert!(one(&tree, &t, &x));
    let mut other = t.clone();
    other.key = "9A".into();
    assert!(!one(&tree, &other, &x), "the NONE group rejects key 9A");
    let mut slow = t.clone();
    slow.bpm = 100.0;
    assert!(!one(&tree, &slow, &x), "the ANY group needs its inner ALL or a 5-star rating");
}

fn lib_data() -> Data {
    // ids 1..=5 with distinct artist / title / rating / bpm / key / year
    let mut a = track(1);
    (a.artist, a.title, a.rating, a.bpm, a.key) = ("zed".into(), "b".into(), 5, 140.0, "8A".into());
    let mut b = track(2);
    (b.artist, b.title, b.rating, b.bpm, b.key) = ("Alpha".into(), "second".into(), 5, 0.0, "".into());
    let mut c = track(3);
    (c.artist, c.title, c.rating, c.bpm, c.key) = ("alpha".into(), "First".into(), 5, 120.0, "9A".into());
    let mut d = track(4);
    (d.rating, d.bpm, d.key) = (2, 130.0, "1B".into());
    let mut e = track(5);
    (e.rating, e.bpm, e.key) = (4, 100.0, "8B".into());
    let years = [1999, 2005, 0, 2010, 2001];
    let extras = years.iter().map(|y| TrackExtra { year: *y, ..extra() }).collect();
    let mut d = data(vec![a, b, c, d, e], extras);
    d.favourites = HashSet::from([1, 2, 3]);
    d.cases = HashMap::from([(9, Some(HashSet::from([1, 4])))]);
    d
}

#[test]
fn default_order_is_artist_then_title_then_id() {
    let d = lib_data();
    let fav = q(vec![Rule::Favourite { value: true }]);
    assert_eq!(ids(&fav, &d), vec![3, 2, 1]);
    let r = evaluate(&q(vec![Rule::RatingMin { stars: 4 }, Rule::BpmRange { min: Some(200.0), max: None }]), &d).unwrap();
    assert!(r.ids.is_empty() && r.matched == 0);
}

#[test]
fn top_level_mode_and_case_membership() {
    let d = lib_data();
    let rules = vec![Rule::InCase { case_id: 9, value: true }, Rule::RatingMin { stars: 4 }];
    assert_eq!(ids(&Query { mode: Mode::All, rules: rules.clone(), ..Default::default() }, &d), vec![1]);
    assert_eq!(ids(&Query { mode: Mode::Any, rules: rules.clone(), ..Default::default() }, &d), vec![4, 5, 3, 2, 1]);
    assert_eq!(ids(&Query { mode: Mode::None, rules, ..Default::default() }, &d), Vec::<i64>::new());
    let not_in = q(vec![Rule::InCase { case_id: 9, value: false }]);
    assert_eq!(ids(&not_in, &d), vec![5, 3, 2]);
}

#[test]
fn a_rule_naming_a_deleted_case_is_an_error_not_a_silent_match_all() {
    let mut d = lib_data();
    d.cases.insert(77, None); // loaded, but the case does not exist
    for id in [77, 1234] {
        let e = evaluate(&q(vec![Rule::InCase { case_id: id, value: false }]), &d).unwrap_err();
        assert!(e.contains("no longer exists"), "{e}");
    }
    // also when the reference is inside a group
    let nested = q(vec![Rule::Group { mode: Mode::Any, items: vec![Rule::PlaysZero, Rule::InCase { case_id: 77, value: true }] }]);
    assert!(evaluate(&nested, &d).is_err());
}

#[test]
fn sorting_any_column_puts_missing_values_last_either_way() {
    let d = lib_data();
    let by = |field, descending| Query { rules: vec![Rule::PlaysZero], sort: Some(Sort { field, descending }), ..Default::default() };
    // BPM: 2 has none
    assert_eq!(ids(&by(SortField::Bpm, false), &d), vec![5, 3, 4, 1, 2]);
    assert_eq!(ids(&by(SortField::Bpm, true), &d), vec![1, 4, 3, 5, 2]);
    // year: 3 has none; equal years fall back to artist/title
    assert_eq!(ids(&by(SortField::Year, false), &d), vec![1, 5, 2, 4, 3]);
    assert_eq!(ids(&by(SortField::Year, true), &d), vec![4, 2, 5, 1, 3]);
    // key sorts round the wheel (1B, 8A, 8B, 9A), unknown last
    assert_eq!(ids(&by(SortField::Key, false), &d), vec![4, 1, 5, 3, 2]);
    assert_eq!(ids(&by(SortField::Key, true), &d), vec![3, 5, 1, 4, 2]);
    // rating ties break on artist, title: 5-star = alpha/First(3), Alpha/second(2), zed(1)
    assert_eq!(ids(&by(SortField::Rating, true), &d), vec![3, 2, 1, 5, 4]);
    assert_eq!(ids(&by(SortField::Rating, false), &d), vec![4, 5, 3, 2, 1]);
    // text: case-insensitive; titles b, second, First, T4, T5
    assert_eq!(ids(&by(SortField::Title, false), &d), vec![1, 3, 2, 4, 5]);
    // a column nobody has filled in (album) leaves the default order
    assert_eq!(ids(&by(SortField::Album, true), &d), vec![4, 5, 3, 2, 1]);
}

#[test]
fn limit_takes_the_first_n_after_sorting_and_reports_how_many_matched() {
    let d = lib_data();
    let top = Query { rules: vec![Rule::PlaysZero], sort: Some(Sort { field: SortField::Bpm, descending: true }), limit: Some(2), ..Default::default() };
    let r = evaluate(&top, &d).unwrap();
    assert_eq!((r.ids.as_slice(), r.matched), (&[1, 4][..], 5));
    let big = Query { limit: Some(50), ..top.clone() };
    let r = evaluate(&big, &d).unwrap();
    assert_eq!((r.ids.len(), r.matched), (5, 5));
    // no rules is allowed only together with a limit, and then takes the whole library
    let all = Query { limit: Some(3), sort: Some(Sort { field: SortField::Year, descending: true }), ..Default::default() };
    assert_eq!(ids(&all, &d), vec![4, 2, 5]);
    assert_eq!(evaluate(&all, &d).unwrap().matched, 5);
    let none_mode = Query { mode: Mode::None, ..all };
    assert_eq!(evaluate(&none_mode, &d).unwrap().matched, 5, "an empty rule list is no filter whatever the mode");
}

#[test]
fn missing_extras_read_as_unknown() {
    let mut d = data(vec![track(1)], vec![]);
    d.extras.clear();
    assert!(ids(&q(vec![Rule::YearRange { min: Some(1), max: None }]), &d).is_empty());
    assert_eq!(ids(&q(vec![text(TextField::Label, TextOp::Is, "")]), &d), vec![1]);
}

#[test]
fn validate_explains_every_refusal() {
    assert!(validate_query(&q(vec![Rule::PlaysZero])).is_ok());
    assert!(validate_query(&q(vec![])).is_err());
    assert!(validate_query(&Query { limit: Some(10), ..Default::default() }).is_ok(), "no rules is fine with a limit");
    for bad in [
        Rule::RatingMin { stars: 0 },
        Rule::RatingMin { stars: 6 },
        Rule::PlaysMin { n: 0 },
        Rule::PlayedWithin { days: 0 },
        Rule::BpmRange { min: None, max: None },
        Rule::BpmRange { min: Some(140.0), max: Some(120.0) },
        Rule::BpmRange { min: Some(-1.0), max: None },
        Rule::BpmRange { min: Some(f64::NAN), max: None },
        Rule::Key { key: "x".into() },
        Rule::KeyCompatible { key: String::new() },
        Rule::Genre { text: "  ".into() },
        text(TextField::Label, TextOp::Contains, "  "),
        Rule::YearRange { min: None, max: None },
        Rule::YearRange { min: Some(0), max: None },
        Rule::YearRange { min: Some(2010), max: Some(2000) },
        Rule::AddedWithin { days: 0 },
        Rule::AddedBefore { days: -3 },
        Rule::DurationRange { min_secs: None, max_secs: None },
        Rule::DurationRange { min_secs: Some(-1), max_secs: None },
        Rule::DurationRange { min_secs: Some(300), max_secs: Some(200) },
        Rule::PlaysRange { min: None, max: None },
        Rule::PlaysRange { min: Some(5), max: Some(2) },
        Rule::PlaysRange { min: Some(-1), max: None },
        Rule::LastPlayedBefore { days: 0, include_never: true },
        Rule::Format { value: " ".into() },
        Rule::InCase { case_id: -1, value: true },
        Rule::Group { mode: Mode::All, items: vec![] },
        Rule::Group { mode: Mode::Any, items: vec![Rule::PlaysMin { n: 0 }] },
    ] {
        assert!(validate_query(&q(vec![bad.clone()])).is_err(), "{bad:?}");
    }
    assert!(validate_query(&q(vec![Rule::BpmRange { min: Some(120.0), max: Some(120.0) }, Rule::Key { key: "Am".into() }])).is_ok());
    assert!(validate_query(&q(vec![Rule::YearRange { min: Some(1990), max: Some(1990) }, Rule::PlaysRange { min: Some(0), max: Some(0) }])).is_ok());
    for limit in [0, MAX_LIMIT + 1] {
        assert!(validate_query(&Query { limit: Some(limit), ..q(vec![Rule::PlaysZero]) }).is_err(), "limit {limit}");
    }
    assert!(validate_query(&Query { limit: Some(MAX_LIMIT), ..q(vec![Rule::PlaysZero]) }).is_ok());
}

#[test]
fn nesting_and_size_are_bounded() {
    let nest = |levels: usize| {
        let mut r = Rule::PlaysZero;
        for _ in 0..levels {
            r = Rule::Group { mode: Mode::All, items: vec![r] };
        }
        r
    };
    assert!(validate_query(&q(vec![nest(MAX_DEPTH - 1)])).is_ok(), "the top list is depth 1");
    assert!(validate_query(&q(vec![nest(MAX_DEPTH)])).is_err());
    assert!(validate_query(&q(vec![nest(500)])).is_err(), "a hostile file is refused, not recursed into");
    let many = vec![Rule::PlaysZero; MAX_NODES];
    assert!(validate_query(&q(many.clone())).is_ok());
    let mut too_many = many;
    too_many.push(Rule::PlaysZero);
    assert!(validate_query(&q(too_many)).is_err());
    // groups count too
    let groups = vec![Rule::Group { mode: Mode::All, items: vec![Rule::PlaysZero] }; MAX_NODES / 2];
    assert!(validate_query(&q(groups.clone())).is_ok(), "100 groups + their 100 rules is exactly the cap");
    let mut one_more = groups;
    one_more.push(Rule::PlaysZero);
    assert!(validate_query(&q(one_more)).is_err(), "groups and the rules inside them all count");
}

#[test]
fn rules_round_trip_through_json_in_the_shape_the_ui_sends() {
    let json = r#"{"mode":"any","sort":{"field":"date_added","descending":true},"limit":50,"rules":[
        {"kind":"rating_min","stars":4},{"kind":"plays_zero"},{"kind":"bpm_range","min":120,"max":null},{"kind":"bpm_range"},
        {"kind":"key_compatible","key":"8A"},{"kind":"played_within","days":30},
        {"kind":"text","field":"remixer","op":"starts_with","text":"x"},
        {"kind":"year_range","min":1990},{"kind":"added_within","days":7},{"kind":"added_before","days":365},
        {"kind":"duration_range","min_secs":60,"max_secs":null},{"kind":"format","value":"FLAC"},
        {"kind":"in_case","case_id":3,"value":false},{"kind":"plays_range","min":0,"max":2},
        {"kind":"last_played_before","days":90},
        {"kind":"group","mode":"none","items":[{"kind":"favourite","value":true},{"kind":"group","mode":"all","items":[{"kind":"plays_zero"}]}]}]}"#;
    let query: Query = serde_json::from_str(json).unwrap();
    assert_eq!(query.mode, Mode::Any);
    assert_eq!(query.sort, Some(Sort { field: SortField::DateAdded, descending: true }));
    assert_eq!(query.limit, Some(50));
    assert_eq!(query.rules[0], Rule::RatingMin { stars: 4 });
    assert_eq!(query.rules[2], Rule::BpmRange { min: Some(120.0), max: None });
    assert_eq!(query.rules[3], Rule::BpmRange { min: None, max: None });
    assert_eq!(query.rules[6], text(TextField::Remixer, TextOp::StartsWith, "x"));
    assert_eq!(query.rules[7], Rule::YearRange { min: Some(1990), max: None });
    assert_eq!(query.rules[14], Rule::LastPlayedBefore { days: 90, include_never: true }, "include_never defaults to on");
    assert!(matches!(&query.rules[15], Rule::Group { mode: Mode::None, items } if items.len() == 2));
    assert!(validate_query(&query).is_err(), "the empty BPM rule (index 3) is read but not accepted");
    let mut valid = query.clone();
    valid.rules.remove(3);
    assert!(validate_query(&valid).is_ok());
    let back: Query = serde_json::from_str(&serde_json::to_string(&query).unwrap()).unwrap();
    assert_eq!(back, query);
    assert!(serde_json::from_str::<Rule>(r#"{"kind":"nope"}"#).is_err());
    assert!(serde_json::from_str::<Rule>(r#"{"kind":"text","field":"bogus","op":"is","text":"a"}"#).is_err());
    // order and limit are optional, and `descending` defaults to ascending
    let bare: Query = serde_json::from_str(r#"{"rules":[],"sort":{"field":"bpm"}}"#).unwrap();
    assert_eq!((bare.mode, bare.sort, bare.limit), (Mode::All, Some(Sort { field: SortField::Bpm, descending: false }), None));
}

#[test]
fn migrate_rewrites_the_old_genre_rule_everywhere_and_is_idempotent() {
    let mut rules = vec![
        Rule::Genre { text: "tech".into() },
        Rule::Group { mode: Mode::Any, items: vec![Rule::PlaysZero, Rule::Group { mode: Mode::All, items: vec![Rule::Genre { text: " house".into() }] }] },
    ];
    migrate(&mut rules);
    assert_eq!(rules[0], text(TextField::Genre, TextOp::Contains, "tech"));
    let Rule::Group { items, .. } = &rules[1] else { panic!() };
    let Rule::Group { items: inner, .. } = &items[1] else { panic!() };
    assert_eq!(inner[0], text(TextField::Genre, TextOp::Contains, " house"));
    let once = rules.clone();
    migrate(&mut rules);
    assert_eq!(rules, once);
    // same answer before and after for a track
    let t = track(1);
    let x = extra();
    assert_eq!(one(&Rule::Genre { text: "tech".into() }, &t, &x), one(&once[0], &t, &x));
    assert_eq!(one(&Rule::Genre { text: " HOU".into() }, &t, &x), one(&text(TextField::Genre, TextOp::Contains, " HOU"), &t, &x));
}

#[test]
fn referenced_cases_walks_groups() {
    let rules = vec![
        Rule::InCase { case_id: 3, value: true },
        Rule::Group { mode: Mode::Any, items: vec![Rule::InCase { case_id: 4, value: false }, Rule::Group { mode: Mode::All, items: vec![Rule::InCase { case_id: 3, value: false }] }] },
    ];
    let mut ids = HashSet::new();
    referenced_cases(&rules, &mut ids);
    assert_eq!(ids, HashSet::from([3, 4]));
}

#[test]
fn the_preview_gets_the_value_an_order_is_based_on_when_the_table_has_no_column_for_it() {
    let d = lib_data();
    let by = |field| Query { rules: vec![Rule::PlaysZero], sort: Some(Sort { field, descending: false }), limit: Some(3), ..Default::default() };
    let r = evaluate(&by(SortField::Year), &d).unwrap();
    assert_eq!((r.ids.as_slice(), r.values.as_slice()), (&[1, 5, 2][..], &["1999".to_string(), "2001".into(), "2005".into()][..]));
    let r = evaluate(&by(SortField::Rating), &d).unwrap();
    assert_eq!(r.values, vec!["\u{2605}\u{2605}".to_string(), "\u{2605}\u{2605}\u{2605}\u{2605}".into(), "\u{2605}\u{2605}\u{2605}\u{2605}\u{2605}".into()]);
    let r = evaluate(&by(SortField::LastPlayed), &d).unwrap();
    assert_eq!(r.values, vec!["never".to_string(); 3], "no last-played date at all");
    // NOW is day 1000 after 1970-01-01 (1972-09-27); the fixture's tracks were added ten days earlier
    assert_eq!(evaluate(&by(SortField::DateAdded), &d).unwrap().values, vec!["1972-09-17".to_string(); 3]);
    // columns the table already shows, and the default order, carry no extra values
    assert!(evaluate(&by(SortField::Bpm), &d).unwrap().values.is_empty());
    assert!(evaluate(&q(vec![Rule::PlaysZero]), &d).unwrap().values.is_empty());
}

#[test]
fn dates_are_formatted_as_utc_calendar_dates() {
    for (ms, want) in [
        (1, "1970-01-01"),
        (1_700_000_000_000, "2023-11-14"),
        (951_782_400_000, "2000-02-29"), // leap day
        (4_102_444_799_000, "2099-12-31"),
        (1_709_251_199_999, "2024-02-29"), // last millisecond of a leap day
        (1_709_251_200_000, "2024-03-01"),
    ] {
        let q = Query { rules: vec![Rule::PlaysZero], sort: Some(Sort { field: SortField::DateAdded, descending: false }), ..Default::default() };
        let mut d = data(vec![track(1)], vec![TrackExtra { date_added: ms, ..extra() }]);
        d.now_ms = ms;
        assert_eq!(evaluate(&q, &d).unwrap().values, vec![want.to_string()], "{ms}");
    }
}
