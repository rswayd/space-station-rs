//! Console lines, the summary, and the JSON and JUnit XML reports.

use std::path::Path;

use serde_json::{Value, json};

use crate::run::CaseResult;

/// One line per case, then a line per failed test with its message.
pub fn progress_line(r: &CaseResult) -> String {
    let passed = r.tests.iter().filter(|t| t.failure.is_none()).count();
    let mut out = format!(
        "{} {:<24} {}/{} tests  {:6.2}s  worker {}{}",
        if r.passed() { "PASS" } else { "FAIL" },
        r.name,
        passed,
        r.tests.len(),
        r.secs,
        r.worker,
        if r.exclusive { " (exclusive)" } else { "" },
    );
    if let Some(e) = &r.error {
        out.push_str(&format!("\n  case error: {e}"));
    }
    for t in &r.tests {
        match &t.failure {
            None => out.push_str(&format!("\n  ok   {} ({:.2}s)", t.name, t.secs)),
            Some(f) => out.push_str(&format!("\n  FAIL {}: {f}", t.name)),
        }
    }
    if !r.passed() {
        out.push_str(&format!("\n  log: {}", r.log.display()));
    }
    out
}

pub fn summary(results: &[CaseResult], wall_secs: f64, jobs: usize) -> String {
    let tests: usize = results.iter().map(|r| r.tests.len()).sum();
    let failed_tests: usize = results
        .iter()
        .map(|r| r.tests.iter().filter(|t| t.failure.is_some()).count())
        .sum();
    let failed_cases: Vec<&str> = results
        .iter()
        .filter(|r| !r.passed())
        .map(|r| r.name.as_str())
        .collect();
    format!(
        "{} cases, {} tests: {} tests failed, {} cases failed{}; wall {wall_secs:.2}s with --jobs {jobs}",
        results.len(),
        tests,
        failed_tests,
        failed_cases.len(),
        if failed_cases.is_empty() {
            String::new()
        } else {
            format!(" ({})", failed_cases.join(", "))
        }
    )
}

pub fn json(results: &[CaseResult], wall_secs: f64, jobs: usize) -> Value {
    let cases: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "file": r.file,
                "exclusive": r.exclusive,
                "worker": r.worker,
                "passed": r.passed(),
                "error": r.error,
                "secs": r.secs,
                "log": r.log,
                "tests": r.tests.iter().map(|t| json!({
                    "name": t.name,
                    "passed": t.failure.is_none(),
                    "failure": t.failure,
                    "secs": t.secs,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "jobs": jobs, "wall_secs": wall_secs, "cases": cases })
}

fn xml(text: &str) -> String {
    text.chars()
        .filter(|&c| c == '\n' || c == '\t' || c >= ' ')
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// JUnit XML: a `testsuite` per case; a case error fails every test of it.
pub fn junit(results: &[CaseResult]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"ss13-bots\">\n",
    );
    for r in results {
        let failures = if r.error.is_some() {
            r.tests.len().max(1)
        } else {
            r.tests.iter().filter(|t| t.failure.is_some()).count()
        };
        out.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{failures}\" time=\"{:.3}\">\n",
            xml(&r.name),
            r.tests.len().max(1),
            r.secs
        ));
        if let Some(e) = &r.error {
            out.push_str(&format!(
                "    <testcase classname=\"{0}\" name=\"(case)\"><failure message=\"case did not start\">{1}</failure></testcase>\n",
                xml(&r.name),
                xml(e)
            ));
        }
        for t in &r.tests {
            out.push_str(&format!(
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                xml(&r.name),
                xml(&t.name),
                t.secs
            ));
            match &t.failure {
                None => out.push_str("/>\n"),
                Some(f) => out.push_str(&format!(
                    "><failure message=\"step failed\">{}</failure></testcase>\n",
                    xml(f)
                )),
            }
        }
        out.push_str("  </testsuite>\n");
    }
    out.push_str("</testsuites>\n");
    out
}

pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::TestResult;

    #[test]
    fn junit_escapes_and_counts() {
        let r = CaseResult {
            name: "doors".into(),
            file: "tests/cases/doors.case".into(),
            exclusive: false,
            worker: 0,
            error: None,
            tests: vec![
                TestResult {
                    name: "ok".into(),
                    failure: None,
                    secs: 0.5,
                },
                TestResult {
                    name: "bad".into(),
                    failure: Some("expected `a < 1` & \"b\"".into()),
                    secs: 1.0,
                },
            ],
            secs: 2.0,
            log: "logs/doors.log".into(),
        };
        let x = junit(std::slice::from_ref(&r));
        assert!(x.contains("tests=\"2\" failures=\"1\""));
        assert!(x.contains("expected `a &lt; 1` &amp; &quot;b&quot;"));
        let j = json(&[r], 3.0, 2);
        assert_eq!(j["cases"][0]["tests"][1]["passed"], false);
        assert_eq!(j["jobs"], 2);
    }
}
