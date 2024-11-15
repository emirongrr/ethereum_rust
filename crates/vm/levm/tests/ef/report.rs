use colored::Colorize;
use std::fmt;

#[derive(Debug, Default)]
pub struct EFTestsReport {
    passed: u64,
    failed: u64,
    total: u64,
    passed_tests: Vec<String>,
    failed_tests: Vec<(String, String)>,
}

impl EFTestsReport {
    pub fn register_pass(&mut self, test_name: &str) {
        self.passed += 1;
        self.passed_tests.push(test_name.to_string());
        self.total += 1;
    }

    pub fn register_fail(&mut self, test_name: &str, reason: &str) {
        self.failed += 1;
        self.failed_tests
            .push((test_name.to_owned(), reason.to_owned()));
        self.total += 1;
    }
}

impl fmt::Display for EFTestsReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let title = "Ethereum Foundation Tests Run Report".bold();
        let passed = format!("{} passed", self.passed).green().bold();
        let failed = format!("{} failed", self.failed).red().bold();
        let total = format!("{} total run", self.total).blue().bold();
        write!(f, "{title}: {passed} {failed} {total}",)
    }
}
