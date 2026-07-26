let meta = #{
    name: "cuni-growth-pipeline",
    description: "Scout tech conversations then execute CuNi growth: ranked opportunities + ready posts/actions",
    when_to_use: "Daily or on-demand CuNi user growth: conversation scout then marketing exec",
    phases: [
        #{ title: "Scout", detail: "Mine HN/Reddit/X/GitHub for comment opportunities" },
        #{ title: "Execute", detail: "Growth exec turns opportunities into posts, drafts, logs" },
        #{ title: "Synthesize", detail: "Human action queue" },
    ],
};

// Optional: args.date as YYYY-MM-DD for artifact names; default fixed stamp from args or "latest"
let date_stamp = if args == () || args.date == () { "latest" } else { args.date };
let max_posts = if args == () || args.max_posts == () { 0 } else { args.max_posts };

phase("Scout");
log("Starting conversation scout for CuNi…");

let scout_prompt = "";
scout_prompt += "You are the cuni-conversation-scout agent. ";
scout_prompt += "Workspace: /home/cee/projects/cuni. ";
scout_prompt += "Scan HN, Reddit, X, GitHub, Lobsters for conversations where CuNi is on-topic ";
scout_prompt += "(multi-target, transpile, polyglot, agent codegen, exactness, py/go/js interop). ";
scout_prompt += "Write the FULL ranked report with draft comments to: ";
scout_prompt += "/home/cee/projects/cuni/docs/growth/conversation-opportunities-" + date_stamp + ".md ";
scout_prompt += "Also write a short JSON summary via your final message. ";
scout_prompt += "Do NOT post any comments. Be thorough; use web_search, open_page, x_keyword_search in parallel.";

let scout = agent(scout_prompt, #{
    label: "cuni-conversation-scout",
    agent_type: "cuni-conversation-scout",
    capability_mode: "read-write",
});

if scout == () || !scout.success {
    log("Scout failed or returned empty — growth will use prior artifacts if any");
} else {
    log("Scout completed");
}

phase("Execute");
log("Starting growth exec…");

let growth_prompt = "";
growth_prompt += "You are the cuni-growth-exec agent. Workspace: /home/cee/projects/cuni. ";
growth_prompt += "Read docs/OUTREACH.md and docs/growth/conversation-opportunities-" + date_stamp + ".md ";
growth_prompt += "(if missing, try conversation-opportunities-latest.md or the newest conversation-opportunities-*.md). ";
growth_prompt += "Also read docs/growth/posts-ready-* if present. ";
growth_prompt += "Tasks: (1) Merge top P0/P1 scout drafts into docs/growth/action-queue-" + date_stamp + ".md ";
growth_prompt += "with final copy-paste comments and post bodies. ";
growth_prompt += "(2) Refresh Show HN / Reddit / TWiR paste pack in docs/growth/posts-ready-" + date_stamp + ".md. ";
growth_prompt += "(3) Append session log to docs/OUTREACH.md. ";
growth_prompt += "(4) Do NOT post publicly and do NOT send bulk cold email unless max_posts allows and only legitimate tip lines. ";
if max_posts != 0 {
    growth_prompt += "max_posts is set; still prefer human submit for HN/Reddit. ";
}
growth_prompt += "Return a concise report of files written and top 5 human clicks.";

let growth = agent(growth_prompt, #{
    label: "cuni-growth-exec",
    agent_type: "cuni-growth-exec",
    capability_mode: "read-write",
});

if growth == () || !growth.success {
    log("Growth exec failed");
    complete(#{
        ok: false,
        scout_ok: scout != () && scout.success,
        growth_ok: false,
        date: date_stamp,
    });
}

phase("Synthesize");
log("Pipeline complete — human action queue ready");

complete(#{
    ok: true,
    scout_ok: scout != () && scout.success,
    growth_ok: growth != () && growth.success,
    date: date_stamp,
    opportunities: "docs/growth/conversation-opportunities-" + date_stamp + ".md",
    action_queue: "docs/growth/action-queue-" + date_stamp + ".md",
    posts: "docs/growth/posts-ready-" + date_stamp + ".md",
    next: "Open action-queue file and submit Show HN + post P0 comments",
});
