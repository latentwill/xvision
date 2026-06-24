# workflow
- Verify changes locally (cargo test + pnpm typecheck/build) before merging — do not gate merges on GitHub Actions CI. Confidence: 0.85
- Merge all open PRs before picking up new work or starting new tasks. Confidence: 0.80
- Do not deploy or build Docker images unless the user explicitly asks for it. Confidence: 0.80
- Use graphify to analyze code structure, find dead/divergent code paths, and investigate issues before refactoring. Confidence: 0.70
- Produce a ce-plan (change-execution plan) before starting implementation work on new features or complex changes. Confidence: 0.70
- Create a PR immediately upon completing any feature, fix, or task — do not leave completed work un-PR'd. Confidence: 0.90
- Route complex fixes and structured tasks through the 100x pipeline rather than handling them inline. Confidence: 0.85
- Subagent independent tasks in parallel for efficiency rather than working through them sequentially. Confidence: 0.75
- Always verify git state (current branch, last commit SHA, dirty/uncommitted files) before starting new work. Confidence: 0.70
