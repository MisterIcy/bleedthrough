## ADDED Requirements

### Requirement: Every ledger entry is analysed
Each entry in the survivor ledger SHALL state a specific reason that explains why its mutant is tolerated, and the ledger SHALL NOT admit entries marked as unanalysed or as recorded debt. The ledger loader SHALL refuse such an entry, in any file, and the refusal SHALL name the entry. A reason that only restates that the mutant survived, or that it has not been analysed, SHALL be refused.

#### Scenario: Unanalysed entry
- **WHEN** the ledger contains an entry marked as recorded baseline debt
- **THEN** loading the ledger fails and names the entry

#### Scenario: Reason restates the survival
- **WHEN** an entry's reason says only that the mutant survives or has not been analysed
- **THEN** loading the ledger fails and names the entry

#### Scenario: Reasoned entry
- **WHEN** an entry in non-critical code gives a specific reason, such as a limit tested elsewhere or an equivalent mutant
- **THEN** the ledger accepts it
