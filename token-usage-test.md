# Token Usage Test: astrolabe-mcp Skill

This document contains a standardized test suite to measure token usage with and without the astrolabe-mcp skill activated.

## Test Methodology

1. **Baseline Run** (without skill):
   - Do NOT activate the astrolabe-mcp skill
   - Ask each question in order
   - Record the token count displayed in Kiro after each response
   - Note the cumulative tokens used

2. **Skill Run** (with skill):
   - Activate the astrolabe-mcp skill using `discloseContext` → astrolabe-mcp
   - Ask the same questions in the same order
   - Record the token count displayed in Kiro after each response
   - Note the cumulative tokens used

3. **Analysis**:
   - Compare total tokens for baseline vs. skill run
   - Calculate percentage savings: `(baseline - skill) / baseline * 100`
   - Note which questions benefit most from the skill

## Test Questions

### Question 1: Basic Tool Discovery
**Prompt:**
```
How do I list all available tools in the astrolabe-mcp server?
```

**Expected Response Type:** Command example or explanation of discovery process

---

### Question 2: Search for Symbols
**Prompt:**
```
Show me how to search for all functions named "parse" in the codebase.
```

**Expected Response Type:** Command with parameters and explanation

---

### Question 3: File Outline
**Prompt:**
```
What's the command to get a compact outline of all symbols in src/indexer.rs?
```

**Expected Response Type:** Specific command with format option

---

### Question 4: Symbol Implementation
**Prompt:**
```
How do I retrieve the source code for the build_qualified_name function?
```

**Expected Response Type:** Command with qualified name syntax

---

### Question 5: Language-Specific Search
**Prompt:**
```
Find all Elixir symbols in the workspace and show me the command.
```

**Expected Response Type:** Command with language filter

---

### Question 6: Text Search
**Prompt:**
```
How do I search for all occurrences of "defmodule" in the code?
```

**Expected Response Type:** Regex search command

---

### Question 7: File Content
**Prompt:**
```
What's the command to read the contents of src/main.rs?
```

**Expected Response Type:** File reading command

---

### Question 8: Workspace Overview
**Prompt:**
```
How do I get an overview of all indexed files and their top-level symbols?
```

**Expected Response Type:** Workspace overview command

---

### Question 9: Multi-Symbol Retrieval
**Prompt:**
```
Show me how to get the source code for multiple symbols at once.
```

**Expected Response Type:** Batch symbol retrieval command

---

### Question 10: Complex Query
**Prompt:**
```
I need to find all struct definitions in Rust files, but only show me the name, kind, and file path. What's the command?
```

**Expected Response Type:** Search command with field filtering

---

## Recording Template

### Baseline Run (No Skill)

| Question | Response Tokens | Cumulative Tokens | Notes |
|----------|-----------------|-------------------|-------|
| Q1 | | | |
| Q2 | | | |
| Q3 | | | |
| Q4 | | | |
| Q5 | | | |
| Q6 | | | |
| Q7 | | | |
| Q8 | | | |
| Q9 | | | |
| Q10 | | | |
| **TOTAL** | | | |

### Skill Run (With astrolabe-mcp Skill)

| Question | Response Tokens | Cumulative Tokens | Notes |
|----------|-----------------|-------------------|-------|
| Q1 | | | |
| Q2 | | | |
| Q3 | | | |
| Q4 | | | |
| Q5 | | | |
| Q6 | | | |
| Q7 | | | |
| Q8 | | | |
| Q9 | | | |
| Q10 | | | |
| **TOTAL** | | | |

## Analysis

**Baseline Total Tokens:** ___________

**Skill Run Total Tokens:** ___________

**Tokens Saved:** ___________

**Percentage Savings:** ___________

**Questions with Highest Savings:**
1. 
2. 
3. 

**Questions with Lowest Savings:**
1. 
2. 
3. 

**Observations:**

---

## Notes

- Token counts are displayed in Kiro's interface at the bottom of the chat
- Each question should be asked independently (don't combine multiple questions)
- Wait for the full response before recording the token count
- The skill context is loaded once at the start of the skill run, so early questions may show higher savings
- Later questions may show diminishing returns as the conversation context grows
