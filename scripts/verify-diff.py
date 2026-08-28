#!/usr/bin/env python3
import sys
import re
import subprocess

def strip_comments(line):
    """Safely strips bash-style trailing comments, respecting quotes."""
    i = 0
    in_dquote = False
    in_squote = False
    while i < len(line):
        c = line[i]
        if c == '\\':
            i += 2
            continue
        if c == '"' and not in_squote:
            in_dquote = not in_dquote
        elif c == "'" and not in_dquote:
            in_squote = not in_squote
        elif c == '#' and not in_dquote and not in_squote:
            # It's a comment if it's the first char or preceded by whitespace
            if i == 0 or line[i-1].isspace():
                return line[:i].strip()
        i += 1
    return line.strip()

def get_echo_skeleton(line):
    """
    Extracts the 'logic footprint' of an echo statement.
    Returns a list of variables, subshells, and operators.
    If it returns an empty list [], it means the echo is purely static text.
    Returns None if the line is not an echo statement.
    """
    s = line.strip()
    if s == 'echo':
        return []
    if not (s.startswith('echo ') or s.startswith('echo\t') or s.startswith('echo"') or s.startswith("echo'")):
        return None

    skeleton = []
    i = 4  # skip 'echo'
    in_dquote = False
    in_squote = False

    while i < len(s):
        c = s[i]
        if c == '\\':
            i += 2
            continue
        if c == '"' and not in_squote:
            in_dquote = not in_dquote
            i += 1
            continue
        if c == "'" and not in_dquote:
            in_squote = not in_squote
            i += 1
            continue

        # Extract Variables and Subshells
        if c == '$' and not in_squote:
            start = i
            if i + 1 < len(s) and s[i+1] == '(':
                depth = 1
                i += 2
                while i < len(s) and depth > 0:
                    if s[i] == '\\':
                        i += 2
                        continue
                    if s[i] == '(': depth += 1
                    elif s[i] == ')': depth -= 1
                    i += 1
                skeleton.append(s[start:i])
                continue
            elif i + 1 < len(s) and s[i+1] == '{':
                i += 2
                while i < len(s) and s[i] != '}':
                    i += 1
                if i < len(s): i += 1
                skeleton.append(s[start:i])
                continue
            else:
                i += 1
                while i < len(s) and (s[i].isalnum() or s[i] == '_'):
                    i += 1
                skeleton.append(s[start:i])
                continue

        # Extract Backtick Subshells
        if c == '`' and not in_squote:
            start = i
            i += 1
            while i < len(s) and s[i] != '`':
                if s[i] == '\\': i += 1
                i += 1
            if i < len(s): i += 1
            skeleton.append(s[start:i])
            continue

        # Extract Shell Operators and everything after them (e.g. `> file.txt`)
        if c in ('>', '|', '&', ';') and not in_dquote and not in_squote:
            skeleton.append(s[i:].strip())
            break

        i += 1

    return skeleton

def process_line(line):
    """
    Transforms a line into its 'logic representation'.
    Returns None if the line should be completely ignored (comments, empty, pure static echo).
    """
    # 1. Em-dash to hyphen normalization
    line = line.replace('—', '-')

    # 2. Strip comments and ignore if empty
    line = strip_comments(line)
    if not line:
        return None

    # 3. Normalize YAML `name:` and `description:` fields
    yaml_match = re.match(r'^(\s*(?:-\s+)?(?:name|description):\s*)(.*)$', line)
    if yaml_match:
        return yaml_match.group(1) + "STR"

    # 4. Normalize `echo` statements to just their logic skeleton
    skel = get_echo_skeleton(line)
    if skel is not None:
        if not skel:
            return None # It's a pure static echo (no variables/operators), ignore it completely!
        return "ECHO_SKEL: " + str(skel)

    return line

def get_diff_text():
    if len(sys.argv) > 1:
        with open(sys.argv[1], 'r', encoding='utf-8') as f:
            return f.read()
    elif not sys.stdin.isatty():
        return sys.stdin.read()
    else:
        try:
            result = subprocess.run(['git', 'diff'], capture_output=True, text=True, check=True)
            return result.stdout
        except subprocess.CalledProcessError as e:
            print(f"Error running git diff: {e}", file=sys.stderr)
            sys.exit(1)
        except FileNotFoundError:
            print("Error: 'git' command not found.", file=sys.stderr)
            sys.exit(1)

def main():
    diff_text = get_diff_text()
    if not diff_text.strip():
        sys.exit(0)

    current_file = None
    violations = []

    deleted_lines = []
    added_lines = []

    def process_block():
        if not deleted_lines and not added_lines:
            return

        # Convert both sides into their abstract logic representations
        del_processed = [p for p in (process_line(l) for l in deleted_lines) if p is not None]
        add_processed = [p for p in (process_line(l) for l in added_lines) if p is not None]

        if del_processed != add_processed:
            violations.append({
                'file': current_file,
                'deleted_original': list(deleted_lines),
                'added_original': list(added_lines),
                'del_processed': del_processed,
                'add_processed': add_processed
            })

        deleted_lines.clear()
        added_lines.clear()

    for line in diff_text.splitlines():
        if line.startswith('--- a/'):
            process_block()
        elif line.startswith('+++ b/'):
            process_block()
            current_file = line[6:]
        elif line.startswith('@@ '):
            process_block()
        elif line.startswith('-') and not line.startswith('--- '):
            deleted_lines.append(line[1:])
        elif line.startswith('+') and not line.startswith('+++ '):
            added_lines.append(line[1:])
        elif line.startswith(' '):
            process_block()
        else:
            process_block()

    process_block()

    # ONLY output if there are actual violations
    if violations:
        print("FAIL: Actual code changes were detected in the diff!\n")
        for v in violations:
            print(f"File: {v['file']}")
            print("--- Original deleted lines ---")
            for l in v['deleted_original']: print(f" - {l}")
            print("+++ Original added lines +++")
            for l in v['added_original']: print(f" + {l}")
            print("--- What the script sees as a CODE mismatch ---")
            print(f"   Deleted logic : {v['del_processed']}")
            print(f"   Added logic   : {v['add_processed']}")
            print("-" * 60)
        sys.exit(1)

    sys.exit(0)

if __name__ == '__main__':
    main()
