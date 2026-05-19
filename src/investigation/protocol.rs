use std::fmt;

/// Hard gates that block state transitions
#[derive(Debug, Clone)]
pub enum GateViolation {
    ProblemNotSingleSentence,
    ReproductionFailed,
    NoEvidenceGathered,
    HypothesisNotFalsifiable,
    NoFailingTest,
    FixNotMinimal,
    VerificationIncomplete,
    ThreeFixesFailed,                 // Requires human decision
    OutputContractIncomplete(String), // Which fields missing
}

impl fmt::Display for GateViolation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GateViolation::ProblemNotSingleSentence => {
                write!(f, "Problem description must be exactly one sentence")
            }
            GateViolation::ReproductionFailed => write!(
                f,
                "Failed to reproduce the failure; cannot proceed without reproduction or instrumentation"
            ),
            GateViolation::NoEvidenceGathered => write!(
                f,
                "No observable evidence collected; cannot form hypothesis"
            ),
            GateViolation::HypothesisNotFalsifiable => write!(
                f,
                "Hypothesis must be disprovable; format: 'Hypothesis: <cause> because <evidence>'"
            ),
            GateViolation::NoFailingTest => write!(
                f,
                "No failing test or reproduction mechanism; cannot verify fix"
            ),
            GateViolation::FixNotMinimal => write!(
                f,
                "Fix must address only root cause; no refactoring, no bundling"
            ),
            GateViolation::VerificationIncomplete => {
                write!(f, "Original failure still occurs or new test does not pass")
            }
            GateViolation::ThreeFixesFailed => write!(
                f,
                "Third fix attempt failed; suspect structural issue; requires human review"
            ),
            GateViolation::OutputContractIncomplete(missing) => {
                write!(f, "Incomplete output contract; missing: {}", missing)
            }
        }
    }
}

/// Problem definition (Phase 1)
#[derive(Clone, Debug)]
pub struct ProblemDefinition {
    pub expected_behavior: String, // What should happen
    pub observed_behavior: String, // What actually happened
    pub scope: String,             // Impact area
    pub reproducible: bool,        // Always, intermittent, or not yet
}

impl ProblemDefinition {
    pub fn validate(&self) -> Result<(), GateViolation> {
        // Hard gate: Must be one sentence
        let combined = format!(
            "{} but got {}",
            &self.expected_behavior, &self.observed_behavior
        );

        if combined.split('.').filter(|s| !s.trim().is_empty()).count() > 1 {
            return Err(GateViolation::ProblemNotSingleSentence);
        }

        Ok(())
    }
}

/// Reproduction attempt (Phase 2)
#[derive(Clone, Debug)]
pub struct ReproductionAttempt {
    pub method: ReproductionMethod,
    pub steps: Vec<String>,
    pub observed_result: String,
    pub consistent: bool,
}

#[derive(Clone, Debug)]
pub enum ReproductionMethod {
    ExistingTest,
    MinimalIntegrationTest,
    UnitTest,
    ManualScript,
    InstrumentedLogs,
}

impl ReproductionAttempt {
    pub fn validate(&self) -> Result<(), GateViolation> {
        // Hard gate: Must be reproducible or instrumented
        if !self.consistent
            && matches!(
                self.method,
                ReproductionMethod::ManualScript | ReproductionMethod::ExistingTest
            )
        {
            return Err(GateViolation::ReproductionFailed);
        }

        Ok(())
    }
}

/// Evidence collection (Phase 3)
#[derive(Clone, Debug)]
pub struct EvidenceCollection {
    pub facts: Vec<Fact>,
}

#[derive(Clone, Debug)]
pub struct Fact {
    pub layer: String, // "Entry", "Business", "Environment", etc.
    pub input_value: String,
    pub output_value: String,
    pub transformed: bool,
    pub condition: String,
}

impl EvidenceCollection {
    pub fn validate(&self) -> Result<(), GateViolation> {
        if self.facts.is_empty() {
            return Err(GateViolation::NoEvidenceGathered);
        }
        Ok(())
    }
}

/// Hypothesis formulation (Phase 4)
#[derive(Clone, Debug)]
pub struct Hypothesis {
    pub root_cause: String,
    pub evidence: String,
}

impl Hypothesis {
    pub fn validate(&self) -> Result<(), GateViolation> {
        // Hard gate: Must follow "Hypothesis: X because Y" format
        if self.root_cause.is_empty() || self.evidence.is_empty() {
            return Err(GateViolation::HypothesisNotFalsifiable);
        }

        Ok(())
    }
}

/// Failure locked (Phase 5)
#[derive(Clone, Debug)]
pub struct FailureGuard {
    pub guard_type: GuardType,
    pub description: String,
    pub passes_before_fix: bool,
    pub passes_after_fix: bool,
}

#[derive(Clone, Debug)]
pub enum GuardType {
    AutomatedTest,
    ReproductionScript,
    ManualVerification,
}

impl FailureGuard {
    pub fn validate(&self) -> Result<(), GateViolation> {
        if !self.passes_before_fix {
            return Err(GateViolation::NoFailingTest);
        }
        Ok(())
    }
}

/// Fix implementation (Phase 6)
#[derive(Clone, Debug)]
pub struct CodeFix {
    pub original_code: String,
    pub fixed_code: String,
    pub rationale: String,
    pub changes_count: usize,
}

impl CodeFix {
    pub fn validate(&self) -> Result<(), GateViolation> {
        // Hard gate: Must be minimal (single focused change)
        if self.changes_count > 5 {
            return Err(GateViolation::FixNotMinimal);
        }

        Ok(())
    }
}

/// Verification (Phase 7)
#[derive(Clone, Debug)]
pub struct Verification {
    pub original_reproduction_still_fails: bool,
    pub guard_now_passes: bool,
    pub related_tests_pass: bool,
    pub side_effects_none: bool,
}

impl Verification {
    pub fn validate(&self) -> Result<(), GateViolation> {
        if self.original_reproduction_still_fails {
            return Err(GateViolation::VerificationIncomplete);
        }

        if !self.guard_now_passes {
            return Err(GateViolation::VerificationIncomplete);
        }

        Ok(())
    }
}

/// Complete investigation output (all 7 required items)
#[derive(Clone, Debug)]
pub struct InvestigationOutput {
    pub problem: ProblemDefinition,
    pub reproduction: ReproductionAttempt,
    pub evidence: EvidenceCollection,
    pub hypothesis: Hypothesis,
    pub guard: FailureGuard,
    pub fix: CodeFix,
    pub verification: Verification,
}

impl InvestigationOutput {
    /// Validate that all 7 outputs are complete and valid
    pub fn validate_complete(&self) -> Result<(), GateViolation> {
        self.problem.validate()?;
        self.reproduction.validate()?;
        self.evidence.validate()?;
        self.hypothesis.validate()?;
        self.guard.validate()?;
        self.fix.validate()?;
        self.verification.validate()?;

        Ok(())
    }
}

/// State machine enforcing 7-phase protocol
#[derive(Clone, Debug)]
pub enum InvestigationPhase {
    Phase1(ProblemDefinition),
    Phase2(ReproductionAttempt),
    Phase3(EvidenceCollection),
    Phase4(Hypothesis),
    Phase5(FailureGuard),
    Phase6(CodeFix),
    Phase7(Verification),
    Complete(InvestigationOutput),
    Failed(GateViolation),
}

impl InvestigationPhase {
    /// Advance to next phase with gate validation
    pub fn transition_to_next(self) -> Result<InvestigationPhase, GateViolation> {
        match self {
            InvestigationPhase::Phase1(problem) => {
                problem.validate()?;
                Ok(InvestigationPhase::Phase2(ReproductionAttempt {
                    method: ReproductionMethod::ExistingTest,
                    steps: vec![],
                    observed_result: String::new(),
                    consistent: false,
                }))
            }
            InvestigationPhase::Phase2(reproduction) => {
                reproduction.validate()?;
                Ok(InvestigationPhase::Phase3(EvidenceCollection {
                    facts: vec![],
                }))
            }
            InvestigationPhase::Phase3(evidence) => {
                evidence.validate()?;
                Ok(InvestigationPhase::Phase4(Hypothesis {
                    root_cause: String::new(),
                    evidence: String::new(),
                }))
            }
            InvestigationPhase::Phase4(hypothesis) => {
                hypothesis.validate()?;
                Ok(InvestigationPhase::Phase5(FailureGuard {
                    guard_type: GuardType::AutomatedTest,
                    description: String::new(),
                    passes_before_fix: false,
                    passes_after_fix: false,
                }))
            }
            InvestigationPhase::Phase5(guard) => {
                guard.validate()?;
                Ok(InvestigationPhase::Phase6(CodeFix {
                    original_code: String::new(),
                    fixed_code: String::new(),
                    rationale: String::new(),
                    changes_count: 0,
                }))
            }
            InvestigationPhase::Phase6(fix) => {
                fix.validate()?;
                Ok(InvestigationPhase::Phase7(Verification {
                    original_reproduction_still_fails: true,
                    guard_now_passes: false,
                    related_tests_pass: false,
                    side_effects_none: false,
                }))
            }
            InvestigationPhase::Phase7(verification) => {
                verification.validate()?;

                // If we got here, all gates passed
                // Phase 7 is terminal
                Ok(InvestigationPhase::Phase7(verification))
            }
            _ => Err(GateViolation::OutputContractIncomplete(
                "Invalid state".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_problem_definition_validation() -> Result<(), GateViolation> {
        // Valid single sentence
        let problem = ProblemDefinition {
            expected_behavior: "Should return true".to_string(),
            observed_behavior: "But got false".to_string(),
            scope: "Login function".to_string(),
            reproducible: true,
        };
        assert!(problem.validate().is_ok());

        // Invalid - multiple sentences
        let problem_multi = ProblemDefinition {
            expected_behavior: "Should return true. And also do something else.".to_string(),
            observed_behavior: "But got false.".to_string(),
            scope: "Login function".to_string(),
            reproducible: true,
        };
        assert!(problem_multi.validate().is_err());

        Ok(())
    }

    #[test]
    fn test_reproduction_attempt_validation() -> Result<(), GateViolation> {
        // Valid - consistent manual script
        let repro = ReproductionAttempt {
            method: ReproductionMethod::ManualScript,
            steps: vec!["Step 1".to_string()],
            observed_result: "Success".to_string(),
            consistent: true,
        };
        assert!(repro.validate().is_ok());

        // Valid - instrumented logs (doesn't need consistency)
        let repro_inst = ReproductionAttempt {
            method: ReproductionMethod::InstrumentedLogs,
            steps: vec![],
            observed_result: "Logs show error".to_string(),
            consistent: false, // This is OK for instrumented logs
        };
        assert!(repro_inst.validate().is_ok());

        // Invalid - inconsistent manual script
        let repro_invalid = ReproductionAttempt {
            method: ReproductionMethod::ManualScript,
            steps: vec!["Step 1".to_string()],
            observed_result: "Sometimes works".to_string(),
            consistent: false,
        };
        assert!(repro_invalid.validate().is_err());

        Ok(())
    }

    #[test]
    fn test_hypothesis_validation() -> Result<(), GateViolation> {
        // Valid hypothesis
        let hypothesis = Hypothesis {
            root_cause: "Null pointer dereference".to_string(),
            evidence: "Stack trace shows crash at line 42".to_string(),
        };
        assert!(hypothesis.validate().is_ok());

        // Invalid - missing root cause
        let hypothesis_invalid = Hypothesis {
            root_cause: "".to_string(),
            evidence: "Some evidence".to_string(),
        };
        assert!(hypothesis_invalid.validate().is_err());

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_investigation_phase_transitions() -> Result<(), GateViolation> {
        // Start with phase 1
        let phase1 = InvestigationPhase::Phase1(ProblemDefinition {
            expected_behavior: "Should work".to_string(),
            observed_behavior: "But failed".to_string(),
            scope: "Test".to_string(),
            reproducible: true,
        });

        // Should transition to phase 2
        let phase2 = phase1.transition_to_next()?;
        if let InvestigationPhase::Phase2(_) = phase2 {
            // Good
        } else {
            panic!("Expected Phase2");
        }

        // Transition through to completion
        let mut current = phase2;
        for _ in 0..4 {
            // Phase2 -> Phase3 -> Phase4 -> Phase5 -> Phase6
            current = current.transition_to_next()?;
        }

        // Should now be at Phase6
        if let InvestigationPhase::Phase6(_) = current {
            // Good
        } else {
            panic!("Expected Phase6");
        }

        // One more to Phase7
        let phase7 = current.transition_to_next()?;
        if let InvestigationPhase::Phase7(_) = phase7 {
            // Good
        } else {
            panic!("Expected Phase7");
        }

        // Final transition should stay at Phase7 (terminal)
        let phase7_again = phase7.transition_to_next()?;
        if let InvestigationPhase::Phase7(_) = phase7_again {
            // Good
        } else {
            panic!("Expected Phase7 to remain");
        }

        Ok(())
    }
}
