#!/usr/bin/env node

/**
 * Streamed Review Queue Orchestrator
 * 
 * 3-phase pipeline to replace monolithic LLM code reviews:
 * 1. Planning: LLM analyzes diff, creates lightweight plan (2048 tokens)
 * 2. Emission: One LLM call per finding (512 tokens each)  
 * 3. Finalization: Deterministic assembly and output (no LLM)
 * 
 * Issue: https://github.com/AI-Daemon/pi-daemon/issues/178
 */

const fs = require('fs');
const path = require('path');

/**
 * Review Queue State Schema
 * 
 * @typedef {Object} QueueState
 * @property {Object} meta - Basic metadata about the review
 * @property {string} meta.review_type - Type of review: architectural|test-quality|configuration
 * @property {string} meta.started_at - ISO timestamp when review started
 * @property {number} meta.diff_size_bytes - Size of the diff file in bytes
 * @property {Object|null} plan - Planning phase result 
 * @property {number} plan.total_findings - Number of findings planned
 * @property {string[]} plan.finding_ids - Array of finding IDs for emission
 * @property {string[]} plan.finding_summaries - Brief summary of each finding
 * @property {Object} plan.scores - Dimension scores {dep_graph: 2, naming: 3, etc}
 * @property {string} plan.verdict_hint - LLM's suggested verdict: PASS|FAIL
 * @property {Object[]} findings - Array of emitted findings
 * @property {string} status - Current pipeline status: planning|emitting|finalizing|complete|error
 * @property {Object} progress - Emission progress tracking
 * @property {number} progress.emitted - Number of findings successfully emitted
 * @property {number} progress.total - Total findings to emit (matches plan.total_findings)
 * @property {string[]} progress.failed_ids - Finding IDs that failed emission
 * @property {string[]} progress.retried_ids - Finding IDs that were retried
 * @property {string|null} verdict - Final verdict: PASS|FAIL (set in finalization)
 * @property {string|null} completed_at - ISO timestamp when review completed
 */

class ReviewQueue {
    /**
     * Create a new review queue
     * 
     * @param {Object} config - Configuration object
     * @param {string} config.reviewType - Review type: architectural|test-quality|configuration  
     * @param {string} config.diffPath - Path to the diff file to review
     * @param {string} config.contextPath - Path to the context file (architecture docs, test standards, etc)
     * @param {string} config.apiKey - OpenRouter API key
     * @param {string} config.model - LLM model to use (default: google/gemini-2.5-flash)
     * @param {string} [config.outputPath] - Path for final result JSON (optional)
     */
    constructor(config) {
        this.config = {
            reviewType: config.reviewType,
            diffPath: config.diffPath,
            contextPath: config.contextPath,
            apiKey: config.apiKey,
            model: config.model || 'google/gemini-2.5-flash',
            outputPath: config.outputPath
        };
        
        // Validate required config
        if (!this.config.reviewType || !this.config.diffPath || !this.config.contextPath || !this.config.apiKey) {
            throw new Error('Missing required config: reviewType, diffPath, contextPath, apiKey are required');
        }
        
        // Initialize state file path
        this.stateFilePath = `/tmp/${this.config.reviewType}_review_queue.json`;
        
        // Initialize empty state
        this.state = this._createEmptyState();
    }

    /**
     * Create empty queue state structure
     * @private
     * @returns {QueueState} Empty state object
     */
    _createEmptyState() {
        const diffStats = this._getDiffStats();
        return {
            meta: {
                review_type: this.config.reviewType,
                started_at: new Date().toISOString(),
                diff_size_bytes: diffStats.size
            },
            plan: null,
            findings: [],
            status: 'planning',
            progress: {
                emitted: 0,
                total: 0,
                failed_ids: [],
                retried_ids: []
            },
            verdict: null,
            completed_at: null
        };
    }

    /**
     * Get diff file statistics
     * @private
     * @returns {Object} Diff statistics {size, exists}
     */
    _getDiffStats() {
        try {
            const stats = fs.statSync(this.config.diffPath);
            return { size: stats.size, exists: true };
        } catch (error) {
            console.warn(`Could not read diff file ${this.config.diffPath}: ${error.message}`);
            return { size: 0, exists: false };
        }
    }

    /**
     * Save current state to disk
     * @private
     */
    _save() {
        try {
            fs.writeFileSync(this.stateFilePath, JSON.stringify(this.state, null, 2));
            console.log(`Queue state saved to ${this.stateFilePath}`);
        } catch (error) {
            console.error(`Failed to save queue state: ${error.message}`);
            // Don't throw - saving is for crash recovery, not critical to operation
        }
    }

    /**
     * Call LLM with retry logic
     * 
     * @private
     * @param {string} systemPrompt - System prompt for the LLM
     * @param {string} userMessage - User message/content to analyze
     * @param {number} maxTokens - Maximum tokens for the response
     * @param {number} [retries=2] - Maximum number of retries
     * @returns {Promise<string>} LLM response content
     */
    async _llmCall(systemPrompt, userMessage, maxTokens, retries = 2) {
        let lastError;
        
        for (let attempt = 0; attempt <= retries; attempt++) {
            try {
                const response = await fetch('https://openrouter.ai/api/v1/chat/completions', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.config.apiKey}`,
                        'HTTP-Referer': 'https://github.com/AI-Daemon/pi-daemon',
                        'X-Title': 'pi-daemon-review-queue'
                    },
                    body: JSON.stringify({
                        model: this.config.model,
                        messages: [
                            { role: 'system', content: systemPrompt },
                            { role: 'user', content: userMessage }
                        ],
                        temperature: 0.1,
                        max_tokens: maxTokens
                    })
                });

                if (!response.ok) {
                    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
                }

                const data = await response.json();
                const content = data.choices?.[0]?.message?.content;
                
                if (!content) {
                    throw new Error('LLM returned empty response');
                }

                return this._cleanJsonResponse(content);

            } catch (error) {
                lastError = error;
                if (attempt < retries) {
                    const delay = Math.pow(2, attempt) * 1000; // Exponential backoff
                    console.warn(`LLM call failed (attempt ${attempt + 1}), retrying in ${delay}ms: ${error.message}`);
                    await new Promise(resolve => setTimeout(resolve, delay));
                } else {
                    console.error(`LLM call failed after ${retries + 1} attempts: ${error.message}`);
                }
            }
        }

        throw lastError;
    }

    /**
     * Clean LLM response by removing markdown code blocks
     * @private
     * @param {string} content - Raw LLM response
     * @returns {string} Cleaned response
     */
    _cleanJsonResponse(content) {
        // Remove markdown code blocks
        return content
            .replace(/^```json\s*/gm, '')
            .replace(/^```\s*/gm, '')
            .replace(/```$/gm, '')
            .trim();
    }

    /**
     * Generate the appropriate system prompt for the review type
     * @private
     * @param {string} phase - Phase: planning|finding
     * @param {Object} [findingContext] - Additional context for finding emission
     * @returns {string} System prompt
     */
    _getSystemPrompt(phase, findingContext = null) {
        const reviewType = this.config.reviewType;
        
        if (phase === 'planning') {
            return this._getPlanningPrompt(reviewType);
        } else if (phase === 'finding' && findingContext) {
            return this._getFindingPrompt(reviewType, findingContext);
        }
        
        throw new Error(`Invalid prompt phase: ${phase}`);
    }

    /**
     * Get planning phase system prompt
     * @private
     * @param {string} reviewType - Type of review
     * @returns {string} Planning prompt
     */
    _getPlanningPrompt(reviewType) {
        const basePrompt = `You are a code reviewer creating a lightweight plan for ${reviewType} review. Your job is to analyze the diff and plan what specific findings you would emit in a detailed review phase.

DO NOT write the full review - just plan it. Output a JSON plan that lists the specific issues you found and would elaborate on later.

Rules:
- Maximum 30 findings (safety cap)
- Each finding gets a unique ID: finding-001, finding-002, etc.
- Include a brief summary for each finding (1 sentence max)
- Score each dimension 0-3 (0=violation, 1=concern, 2=acceptable, 3=excellent)
- Suggest overall verdict based on your analysis

Output JSON only:
{
  "total_findings": N,
  "finding_ids": ["finding-001", "finding-002", ...],
  "finding_summaries": ["Brief summary of finding-001", ...],
  "scores": {"dimension1": N, "dimension2": N, ...},
  "verdict_hint": "PASS|FAIL"
}`;

        // Add review-type specific guidance
        if (reviewType === 'architectural') {
            return basePrompt + '\n\nFocus on: dependency graph compliance, concurrency patterns, error handling, testing coverage, naming conventions, security, documentation.';
        } else if (reviewType === 'test-quality') {
            return basePrompt + '\n\nFocus on: test helper usage, naming patterns, error coverage, edge cases, test isolation, proper assertions, test structure.';
        } else if (reviewType === 'configuration') {
            return basePrompt + '\n\nFocus on: workflow structure, naming clarity, security patterns, documentation quality, maintainability.';
        }
        
        return basePrompt;
    }

    /**
     * Get finding emission phase system prompt
     * @private
     * @param {string} reviewType - Type of review
     * @param {Object} findingContext - Context about the specific finding
     * @returns {string} Finding prompt
     */
    _getFindingPrompt(reviewType, findingContext) {
        return `You are a code reviewer writing ONE specific finding for a ${reviewType} review.

Focus only on: ${findingContext.summary}
Finding ID: ${findingContext.id}

Write a detailed finding with actionable feedback. Be specific about file and line locations when possible.

Output JSON only:
{
  "id": "${findingContext.id}",
  "description": "Detailed description of the issue",
  "file": "path/to/file (optional)",
  "line": N,
  "severity": "error|warning|info"
}`;
    }

    /**
     * Planning Phase: Analyze diff and create lightweight plan
     * @returns {Promise<Object>} Planning result
     */
    async plan() {
        console.log(`Starting planning phase for ${this.config.reviewType} review...`);
        
        try {
            // Read diff and context
            const diff = fs.readFileSync(this.config.diffPath, 'utf8');
            const context = fs.readFileSync(this.config.contextPath, 'utf8');
            
            // Prepare user message
            const userMessage = `${context}\n\n## Diff to Review\n\`\`\`diff\n${diff}\n\`\`\``;
            
            // Get planning prompt
            const systemPrompt = this._getSystemPrompt('planning');
            
            // Call LLM for planning
            const planResponse = await this._llmCall(systemPrompt, userMessage, 2048);
            
            // Parse and validate plan
            let plan;
            try {
                plan = JSON.parse(planResponse);
            } catch (parseError) {
                throw new Error(`Planning response not valid JSON: ${parseError.message}`);
            }
            
            // Validate plan structure
            if (!plan.total_findings || !plan.finding_ids || !plan.finding_summaries) {
                throw new Error('Planning response missing required fields: total_findings, finding_ids, finding_summaries');
            }
            
            if (plan.finding_ids.length !== plan.finding_summaries.length) {
                throw new Error('Mismatch between finding_ids and finding_summaries length');
            }
            
            // Apply safety cap
            if (plan.total_findings > 30) {
                console.warn(`Planned ${plan.total_findings} findings, capping at 30`);
                plan.total_findings = 30;
                plan.finding_ids = plan.finding_ids.slice(0, 30);
                plan.finding_summaries = plan.finding_summaries.slice(0, 30);
            }
            
            // Update state
            this.state.plan = plan;
            this.state.progress.total = plan.total_findings;
            this.state.status = 'emitting';
            this._save();
            
            console.log(`Planning complete: ${plan.total_findings} findings planned`);
            return plan;
            
        } catch (error) {
            this.state.status = 'error';
            this._save();
            throw new Error(`Planning phase failed: ${error.message}`);
        }
    }

    /**
     * Emission Phase: Emit all planned findings one by one
     * @returns {Promise<Object[]>} Array of emitted findings
     */
    async emitAll() {
        console.log('Starting emission phase...');
        
        if (!this.state.plan) {
            throw new Error('Cannot emit findings without a plan - run plan() first');
        }
        
        const findings = [];
        const { finding_ids, finding_summaries } = this.state.plan;
        
        // Read diff and context once for all emissions
        const diff = fs.readFileSync(this.config.diffPath, 'utf8');
        const context = fs.readFileSync(this.config.contextPath, 'utf8');
        const baseUserMessage = `${context}\n\n## Diff to Review\n\`\`\`diff\n${diff}\n\`\`\``;
        
        for (let i = 0; i < finding_ids.length; i++) {
            const findingId = finding_ids[i];
            const summary = finding_summaries[i];
            
            console.log(`Emitting finding ${i + 1}/${finding_ids.length}: ${findingId}`);
            
            try {
                // Get finding-specific prompt
                const findingContext = { id: findingId, summary };
                const systemPrompt = this._getSystemPrompt('finding', findingContext);
                
                // Call LLM for this specific finding
                const findingResponse = await this._llmCall(systemPrompt, baseUserMessage, 512);
                
                // Parse and validate finding
                let finding;
                try {
                    finding = JSON.parse(findingResponse);
                } catch (parseError) {
                    console.warn(`Finding ${findingId} response not valid JSON, creating fallback`);
                    finding = {
                        id: findingId,
                        description: summary, // Use planning summary as fallback
                        file: null,
                        line: null,
                        severity: 'info'
                    };
                }
                
                // Force-correct the ID to planned value
                finding.id = findingId;
                
                // Validate required fields
                if (!finding.description) {
                    finding.description = summary; // Fallback to planning summary
                }
                
                findings.push(finding);
                this.state.findings.push(finding);
                this.state.progress.emitted++;
                
                console.log(`Finding ${findingId} emitted successfully`);
                
            } catch (error) {
                console.warn(`Finding ${findingId} emission failed: ${error.message}`);
                
                // Create skeleton entry for failed findings
                const skeletonFinding = {
                    id: findingId,
                    description: summary, // Use planning summary
                    file: null,
                    line: null,
                    severity: 'info'
                };
                
                findings.push(skeletonFinding);
                this.state.findings.push(skeletonFinding);
                this.state.progress.failed_ids.push(findingId);
                this.state.progress.emitted++; // Still counts as processed
                
                console.log(`Created skeleton entry for failed finding ${findingId}`);
            }
            
            // Save state after each finding (crash recovery)
            this._save();
        }
        
        this.state.status = 'finalizing';
        this._save();
        
        console.log(`Emission complete: ${findings.length} findings emitted, ${this.state.progress.failed_ids.length} failed`);
        return findings;
    }

    /**
     * Finalization Phase: Compute verdict and build final result
     * @returns {Object} Final review result in expected schema
     */
    finalize() {
        console.log('Starting finalization phase...');
        
        if (!this.state.plan || this.state.findings.length === 0) {
            throw new Error('Cannot finalize without plan and findings - run plan() and emitAll() first');
        }
        
        const { plan, findings } = this.state;
        const scores = plan.scores || {};
        
        // Compute verdict using same logic as current system
        // Any dimension 0 → FAIL, average < 2.0 → FAIL
        const scoreValues = Object.values(scores);
        const hasZero = scoreValues.some(score => score === 0);
        const average = scoreValues.length > 0 ? scoreValues.reduce((a, b) => a + b, 0) / scoreValues.length : 2.0;
        
        const verdict = hasZero || average < 2.0 ? 'FAIL' : 'PASS';
        
        // Build result in exact schema expected by downstream steps
        const result = {
            verdict: verdict,
            summary: this._generateSummary(verdict, findings.length),
            checklist_verdict: verdict, // Use same verdict for both
            expert_verdict: verdict,
            scores: scores,
            checks: this._buildChecks(findings),
            issues: this._buildIssues(findings),
            actions: this._buildActions(findings),
            notes: this._generateNotes()
        };
        
        // Update final state
        this.state.verdict = verdict;
        this.state.status = 'complete';
        this.state.completed_at = new Date().toISOString();
        this._save();
        
        console.log(`Finalization complete: ${verdict} verdict with ${findings.length} findings`);
        return result;
    }

    /**
     * Generate summary for the review
     * @private
     * @param {string} verdict - PASS or FAIL
     * @param {number} findingCount - Number of findings
     * @returns {string} Summary text
     */
    _generateSummary(verdict, findingCount) {
        if (verdict === 'PASS') {
            return findingCount === 0 
                ? `${this.config.reviewType} review passed with no issues found`
                : `${this.config.reviewType} review passed with ${findingCount} minor findings`;
        } else {
            return `${this.config.reviewType} review failed with ${findingCount} findings requiring attention`;
        }
    }

    /**
     * Build checks array from findings
     * @private
     * @param {Object[]} findings - Array of findings
     * @returns {Object[]} Checks array
     */
    _buildChecks(findings) {
        const checks = [
            {
                name: 'Queue Processing',
                status: 'pass',
                detail: `Processed ${findings.length} findings via streamed queue`
            }
        ];
        
        if (this.state.progress.failed_ids.length > 0) {
            checks.push({
                name: 'Finding Emission',
                status: 'warning',
                detail: `${this.state.progress.failed_ids.length} findings failed emission but were given skeleton entries`
            });
        }
        
        return checks;
    }

    /**
     * Build issues array from findings
     * @private
     * @param {Object[]} findings - Array of findings
     * @returns {Object[]} Issues array
     */
    _buildIssues(findings) {
        return findings
            .filter(f => f.severity === 'error' || f.severity === 'warning')
            .map(finding => ({
                description: finding.description,
                file: finding.file || undefined,
                line: finding.line || undefined
            }));
    }

    /**
     * Build actions array from findings
     * @private
     * @param {Object[]} findings - Array of findings
     * @returns {Object[]} Actions array
     */
    _buildActions(findings) {
        return findings
            .filter(f => f.severity === 'error')
            .map((finding, index) => ({
                action: `Fix: ${finding.description}`,
                file: finding.file || undefined,
                line: finding.line || undefined
            }));
    }

    /**
     * Generate notes about the review process
     * @private
     * @returns {string} Notes text
     */
    _generateNotes() {
        const notes = [`Streamed review queue processed ${this.state.progress.total} findings in 3 phases.`];
        
        if (this.state.progress.failed_ids.length > 0) {
            notes.push(`${this.state.progress.failed_ids.length} findings failed emission but were recovered with skeleton entries.`);
        }
        
        const duration = this.state.completed_at && this.state.meta.started_at
            ? Math.round((new Date(this.state.completed_at) - new Date(this.state.meta.started_at)) / 1000)
            : 0;
            
        if (duration > 0) {
            notes.push(`Total processing time: ${duration} seconds.`);
        }
        
        return notes.join(' ');
    }

    /**
     * Run the complete 3-phase pipeline
     * @returns {Promise<Object>} Final result
     */
    async run() {
        console.log(`Starting ${this.config.reviewType} review queue pipeline...`);
        
        try {
            await this.plan();
            await this.emitAll();
            const result = this.finalize();
            
            // Write output file if specified
            if (this.config.outputPath) {
                fs.writeFileSync(this.config.outputPath, JSON.stringify(result, null, 2));
                console.log(`Final result written to ${this.config.outputPath}`);
            }
            
            return result;
            
        } catch (error) {
            this.state.status = 'error';
            this._save();
            throw error;
        }
    }
}

// CLI interface when run as script
if (require.main === module) {
    const { program } = require('commander');
    
    program
        .name('review-queue')
        .description('Streamed Review Queue Orchestrator for pi-daemon')
        .version('1.0.0')
        .requiredOption('--type <type>', 'Review type: architectural|test-quality|configuration')
        .requiredOption('--diff <path>', 'Path to diff file to review')
        .requiredOption('--context <path>', 'Path to context file (docs, standards, etc)')
        .requiredOption('--output <path>', 'Path to write final result JSON')
        .option('--model <model>', 'LLM model to use', 'google/gemini-2.5-flash')
        .option('--api-key <key>', 'OpenRouter API key (or set OPENROUTER_API_KEY env var)')
        .parse();

    const options = program.opts();
    
    // Get API key from option or environment
    const apiKey = options.apiKey || process.env.OPENROUTER_API_KEY;
    if (!apiKey) {
        console.error('Error: API key required via --api-key or OPENROUTER_API_KEY environment variable');
        process.exit(1);
    }
    
    // Validate review type
    const validTypes = ['architectural', 'test-quality', 'configuration'];
    if (!validTypes.includes(options.type)) {
        console.error(`Error: Invalid review type '${options.type}'. Must be one of: ${validTypes.join(', ')}`);
        process.exit(1);
    }
    
    // Create and run queue
    const queue = new ReviewQueue({
        reviewType: options.type,
        diffPath: options.diff,
        contextPath: options.context,
        apiKey: apiKey,
        model: options.model,
        outputPath: options.output
    });
    
    queue.run()
        .then((result) => {
            console.log(`\n✅ ${options.type} review completed: ${result.verdict}`);
            console.log(`📄 Result written to ${options.output}`);
            console.log(`📊 State file: /tmp/${options.type}_review_queue.json`);
            
            // Exit with appropriate code
            process.exit(result.verdict === 'PASS' ? 0 : 1);
        })
        .catch((error) => {
            console.error(`\n❌ Review queue failed: ${error.message}`);
            console.log(`📊 State file: /tmp/${options.type}_review_queue.json (may contain partial progress)`);
            process.exit(1);
        });
}

// Export for testing
module.exports = { ReviewQueue };