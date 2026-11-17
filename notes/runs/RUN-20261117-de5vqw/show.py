"""Prints a transcript's tool calls and the assistant's text, one line each (text whole)."""
import json, sys
for line in open(sys.argv[1]):
    v = json.loads(line)
    if v.get("type") != "assistant":
        continue
    for b in v["message"]["content"]:
        if b.get("type") == "tool_use":
            print("TOOL", b["name"], json.dumps(b["input"])[:160])
        elif b.get("type") == "text":
            print("TEXT", b["text"])
