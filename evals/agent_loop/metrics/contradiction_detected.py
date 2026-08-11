import json

def grade(trace):
    """
    Judges whether the agent correctly detected a contradiction during memory consolidation.
    Returns (pass_fail, reason).
    """
    output = trace.get("output", "")
    
    try:
        # Assuming the agent outputs a JSON response with consolidation actions
        response = json.loads(output)
        actions = response.get("actions", [])
        
        has_replace = any(action.get("type") == "replace" for action in actions)
        
        if has_replace:
            return 1, "The agent correctly issued a 'replace' action to resolve the contradiction."
        else:
            return 0, "The agent failed to issue a 'replace' action. It might have incorrectly appended both facts."
            
    except json.JSONDecodeError:
        return 0, "The agent did not output valid JSON."
