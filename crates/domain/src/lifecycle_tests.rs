//! Regression cases from Nsight's retroactive GitLab tracking and paused umbrella tickets.

use swarmception_model::{AgentFile, Issues, MainMerge, Timestamp, parse_file, parse_merge};

use crate::details::{Subject, details};
use crate::factory::{CrateState, scene};
use crate::{Activity, Node, Phase, RowsSpec, Window, tickets};

fn time(s: &str) -> Timestamp {
    Timestamp::parse(s).unwrap()
}

fn files(last: &str) -> Vec<AgentFile> {
    vec![
        parse_file(
            "retro-worker",
            r#"{"event":"session.start","t":"2026-10-13T18:48:35Z"}
{"event":"claim","issue":12,"mr":2,"t":"2026-10-13T18:49:35Z"}
{"event":"work","issue":12,"t":"2026-10-13T18:49:40Z"}
{"event":"submit","issue":12,"mr":2,"t":"2026-10-13T19:39:51Z"}
{"event":"stop","issue":12,"reason":"done","t":"2026-10-13T19:39:52Z"}
{"event":"session.stop","t":"2026-10-13T19:40:52Z"}"#,
        ),
        parse_file("coordinator", &format!(r#"{{"event":"heartbeat","t":"{last}"}}"#)),
    ]
}

fn batch() -> MainMerge {
    parse_merge("997949e414\u{1f}1791922139\u{1f}Merge branch 'batch-1' into 'main'\n\nCloses #12, #3, and #9\n\nSee merge request cmaughan/ns!8").unwrap()
}

#[test]
fn gitlab_batch_closure_recovers_a_missing_merge_for_all_ticket_consumers() {
    let merges = vec![batch()];
    let activity = Activity::with_merges(&files("2026-11-06T13:02:42Z"), &merges);
    let all = tickets(&activity);
    let ticket = &all[0];
    let merged = ticket.merged.as_ref().unwrap();
    assert_eq!(merged.t, time("2026-10-13T20:08:59Z"));
    assert_eq!(merged.commit.as_deref(), Some("997949e414"));
    assert_eq!(merged.pr, Some(8), "the landing batch, not worker MR 2");
    assert!(ticket.unresolved.is_none() && ticket.done.is_none());
    assert_eq!(activity.events(), 7, "no invented tracking events or agents");
    let rows = crate::rows::rows(&activity, &merges, RowsSpec::default());
    let bar = rows
        .iter()
        .flat_map(Node::leaves)
        .filter_map(|n| match n {
            Node::Ranges { bars, .. } => Some(bars),
            _ => None,
        })
        .flatten()
        .find(|b| b.ticket == Some(12) && b.style == crate::rows::Style::Ticket)
        .unwrap();
    assert_eq!(bar.end.0 - bar.start.0, 4764, "79 minutes, not 570 hours");
    assert!(!bar.open);
    assert!(bar.tooltip.iter().any(|s| s.contains("merge recovered from main commit")));
    let factory = scene(&activity, &merges, merged.t, 60);
    assert_eq!(factory.counts.merged, 1);
    let stats = crate::stats::stats(&activity, &all, &merges, Window::default());
    assert_eq!(stats.landed, 1);
    let details = details(&activity, &merges, &Issues::default(), &Subject::Ticket { issue: 12 }).unwrap();
    let entry = details.entries.iter().find(|e| e.what == "merged").unwrap();
    assert_eq!(entry.t, merged.t);
    assert_eq!(entry.goes_to, Some(Subject::Merge { commit: "997949e414".into() }));
}

#[test]
fn incomplete_old_history_is_bounded_and_explained_without_inventing_a_merge() {
    let activity = Activity::of(&files("2026-11-06T13:02:42Z"));
    let all = tickets(&activity);
    let ticket = &all[0];
    let last = time("2026-10-13T19:39:52Z");
    assert_eq!(ticket.unresolved, Some(last));
    assert_eq!(ticket.stretches.last().unwrap().end, last);
    assert!(!ticket.stretches.last().unwrap().open);
    assert!(ticket.merged.is_none() && ticket.done.is_none() && ticket.abandoned.is_none());
    let factory = scene(&activity, &[], time("2026-11-06T13:02:42Z"), 60);
    let shelved = factory.shelf.iter().find(|c| c.issue == 12).unwrap();
    assert_eq!(shelved.state, CrateState::Shelved);
    assert!(shelved.note.as_deref().unwrap().contains("outcome unknown"));
}

#[test]
fn recent_submissions_stay_open_and_a_recorded_long_wait_is_not_truncated() {
    let recent = Activity::of(&files("2026-10-13T20:00:00Z"));
    let all = tickets(&recent);
    assert!(all[0].stretches.last().unwrap().open);
    assert!(all[0].unresolved.is_none() && all[0].done.is_none());
    let mut old = files("2026-11-06T13:02:42Z");
    old.push(parse_file("merger", r#"{"event":"merge","issue":12,"mr":2,"t":"2026-11-06T12:00:00Z"}"#));
    let activity = Activity::with_merges(&old, &[batch()]);
    let all = tickets(&activity);
    assert_eq!(all[0].merged.as_ref().unwrap().t, time("2026-11-06T12:00:00Z"), "explicit tracking wins");
    assert_eq!(all[0].merged.as_ref().unwrap().by, "merger");
    assert_eq!(all[0].stretches.last().unwrap().end, time("2026-11-06T12:00:00Z"));
    assert!(all[0].unresolved.is_none());
}

#[test]
fn a_merge_after_the_last_tracking_event_extends_the_recorded_extent() {
    let merges = [batch()];
    let activity = Activity::with_merges(&files("2026-10-13T19:40:52Z"), &merges);
    assert_eq!(activity.extent.unwrap().1, merges[0].t);
    let stats = crate::stats::stats(&activity, &tickets(&activity), &merges, Window::default());
    assert_eq!(stats.landed, 1);
}

#[test]
fn matching_pr_identity_recovers_a_merge_without_closing_keywords() {
    let merge = parse_merge("abc\u{1f}1791922139\u{1f}Merge pull request #2 from owner/topic").unwrap();
    let activity = Activity::with_merges(&files("2026-11-06T13:02:42Z"), &[merge]);
    let all = tickets(&activity);
    assert_eq!(all[0].merged.as_ref().unwrap().commit.as_deref(), Some("abc"));
}

#[test]
fn the_day_threshold_is_based_on_observations_and_does_not_invent_death() {
    let before = tickets(&Activity::of(&files("2026-10-14T19:39:51Z")));
    assert!(before[0].stretches.last().unwrap().open);
    let after = tickets(&Activity::of(&files("2026-10-14T19:39:52Z")));
    assert!(after[0].unresolved.is_some());
    let silent = [
        parse_file("worker", r#"{"event":"work","issue":1,"t":"2026-10-13T19:00:00Z"}"#),
        parse_file("coordinator", r#"{"event":"heartbeat","t":"2026-11-06T13:00:00Z"}"#),
    ];
    let activity = Activity::of(&silent);
    assert!(activity.agent("worker").unwrap().sessions[0].span.open, "the session is not declared dead");
    let all = tickets(&activity);
    assert!(all[0].unresolved.is_some());
    assert!(all[0].abandoned.is_none());
}

#[test]
fn pauses_and_blocks_end_the_bar_but_work_can_resume() {
    for reason in ["paused", "blocked", "handoff"] {
        let worker = format!(
            r#"{{"event":"claim","issue":257,"mr":262,"t":"2026-10-18T10:10:04Z"}}
{{"event":"stop","issue":257,"reason":"{reason}","t":"2026-10-18T10:49:40Z"}}"#
        );
        let mut files = vec![parse_file("worker", &worker), parse_file("coordinator", r#"{"event":"heartbeat","t":"2026-11-06T13:02:42Z"}"#)];
        let activity = Activity::of(&files);
        let all = tickets(&activity);
        let stopped = all[0].stretches.last().unwrap();
        assert_eq!(
            (stopped.phase, stopped.start, stopped.end, stopped.open),
            (Phase::Stopped, time("2026-10-18T10:49:40Z"), time("2026-10-18T10:49:40Z"), false)
        );
        let factory = scene(&activity, &[], time("2026-10-18T11:00:00Z"), 60);
        assert_eq!(factory.counts.at_benches, 0, "set down, no ghost on a bench");
        assert_eq!(factory.shelf[0].note.as_deref(), Some(format!("stopped: {reason}").as_str()));
        assert_eq!(factory.counts.merged, 0, "stopped does not mean merged");
        files.push(parse_file("resumer", r#"{"event":"work","issue":257,"t":"2026-11-06T13:00:00Z"}"#));
        let all = tickets(&Activity::of(&files));
        assert_eq!(all[0].stretches.last().unwrap().phase, Phase::Working);
        assert!(all[0].stretches.last().unwrap().open);
    }
}

#[test]
fn an_old_merge_does_not_close_new_work_or_leave_an_old_merged_flag() {
    let mut files = files("2026-11-06T13:02:42Z");
    files.push(parse_file("merger", r#"{"event":"merge","issue":12,"mr":2,"t":"2026-10-13T20:08:59Z"}"#));
    files.push(parse_file(
        "new-worker",
        r#"{"event":"work","issue":12,"mr":99,"t":"2026-11-06T13:00:00Z"}
{"event":"submit","issue":12,"mr":99,"t":"2026-11-06T13:01:00Z"}"#,
    ));
    let activity = Activity::with_merges(&files, &[batch()]);
    let all = tickets(&activity);
    assert!(all[0].merged.is_none());
    assert!(all[0].stretches.last().unwrap().open);
    assert_eq!(all[0].pr, Some(99));
}
