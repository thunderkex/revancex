import json

with open('output/BUILD_REPORT.json', 'r', encoding='utf-8') as f:
    report = json.load(f)

print(f"Summary: total={report['summary']['total']}, success={report['summary']['success']}, failed={report['summary']['failed']}")
print(f"Open breakers: {report['summary'].get('open_breakers')}")

print("\n--- SUCCESSFUL APPS ---")
for id, app in report.get('apps', {}).items():
    if app.get('status') == 'success':
        src = app.get('resolved_source')
        ver = app.get('resolved_version')
        out = app.get('output_file')
        arch = app.get('arch_mode')
        print(f"  {id}: source={src}, ver={ver}, arch={arch}, file={out}")

for id, app in report.get('apps', {}).items():
    if app.get('status') != 'success':
        print(f"\n=== {id} (target: {app.get('target_version')}, supported: {app.get('patch_supported_versions')}) ===")
        print("  error:", app.get('error'))
        for att in app.get('attempted_sources', []):
            src = att.get('source')
            outcome = att.get('outcome')
            reason = (att.get('reason') or '')[:80]
            cb = att.get('breaker_state')
            print(f"    - {src}: {outcome} (cb={cb}) {reason}")
