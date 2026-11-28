"""Linux-only evidence helpers for the isolated paired performance job."""
from __future__ import annotations

import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import resource
import statistics
import struct
import subprocess
import time

BUILD_NAMES = re.compile(
    r"^(?:(?:[A-Za-z0-9_]+-)*(?:gcc|g\+\+|cc|c\+\+|clang|clang\+\+|ld|ld\.lld|ld\.gold|ld\.bfd)(?:-[0-9.]+)?"
    r"|yavo|yavo-selfhost|cargo|rustc|zig|rust-lld|collect2|cc1|cc1plus|lld|ninja|make|cmake|meson|ccache|sccache)$")
ENV_NAMES = ("GITHUB_ACTIONS", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_SHA",
             "RUNNER_ENVIRONMENT", "RUNNER_OS", "RUNNER_ARCH", "RUNNER_NAME",
             "ImageOS", "ImageVersion", "YAVO_HOME", "CARGO_BUILD_JOBS")


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def digest(path):
    state = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            state.update(block)
    return state.hexdigest()


def identity(path):
    path = Path(path).resolve()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def runtime_pch_evidence(trace, mode, warmup, cache, expected=None):
    """Bind the observed driver path to one immutable PCH in a fresh lane."""
    if mode not in ("plain", "pch"): raise ValueError("unknown compiler lane")
    if "unavailable" in trace or "rejected" in trace: raise RuntimeError("cache/PCH fallback would invalidate the comparison")
    if mode == "plain":
        if "runtime pch" in trace: raise RuntimeError("direct arm unexpectedly used or attempted a runtime PCH")
        return None
    hits = re.findall(r"runtime pch (hit|miss) ([0-9a-f]+)", trace)
    if len(hits) != 1 or hits[0][0] != ("miss" if warmup else "hit"):
        raise RuntimeError("PCH must miss on the fresh lane warmup and hit on every retained sample")
    key = hits[0][1]
    binding = {"key": key, **identity(Path(cache) / "pch" / key / "yavo_rt.pch")}
    if not warmup and expected != binding: raise RuntimeError("PCH key/path/bytes/SHA changed within a measured lane")
    return binding


def distribution(rows):
    values = sorted(r["elapsed_seconds"] for r in rows)
    return {"samples": len(values), "p50_seconds": statistics.median(values),
            "p95_seconds": values[math.ceil(.95 * len(values)) - 1],
            "min_seconds": values[0], "max_seconds": values[-1],
            "peak_wait4_rss_bytes": max(r["rusage"]["maxrss_kib"] * 1024 for r in rows),
            "median_user_seconds": statistics.median(r["rusage"]["user_seconds"] for r in rows),
            "median_system_seconds": statistics.median(r["rusage"]["system_seconds"] for r in rows)}


def require_runner():
    if not (platform.system() == "Linux" and platform.machine() == "x86_64"
            and os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
            and os.environ.get("GITHUB_RUN_ID") and os.environ.get("GITHUB_SHA")):
        raise RuntimeError("performance samples require a dedicated GitHub-hosted Linux x86_64 job")


def proc_table():
    result = {}
    for path in Path("/proc").glob("[0-9]*/stat"):
        try:
            text = path.read_text(encoding="utf-8")
            start, end = text.index("("), text.rindex(")")
            rest = text[end + 2:].split()
            pid = int(text[:start].strip())
            comm = text[start + 1:end]
            try:
                executable = Path(os.readlink(path.parent / "exe")).name
            except OSError:
                try:
                    executable = Path((path.parent / "cmdline").read_bytes().split(b"\0", 1)[0].decode("utf-8", errors="replace")).name or comm
                except OSError:
                    executable = comm
            result[pid] = {"pid": pid, "ppid": int(rest[1]), "pgrp": int(rest[2]),
                           "comm": comm, "executable": executable, "state": rest[0],
                           "user_ticks": int(rest[11]), "system_ticks": int(rest[12]),
                           "start_ticks": int(rest[19])}
        except (OSError, ValueError, IndexError):
            continue
    return result


def foreign_builds(table, owned_roots=()):
    owned = set(owned_roots)
    while True:
        more = {pid for pid, row in table.items() if row["ppid"] in owned}
        if more <= owned:
            break
        owned |= more
    return [row for pid, row in sorted(table.items()) if pid not in owned and row["state"] not in ("Z", "X")
            and (BUILD_NAMES.fullmatch(row["comm"]) or BUILD_NAMES.fullmatch(row["executable"]))]


def snapshot():
    cpu = {}
    for line in Path("/proc/stat").read_text(encoding="utf-8").splitlines():
        fields = line.split()
        if fields and re.fullmatch(r"cpu[0-9]*", fields[0]):
            cpu[fields[0]] = [int(x) for x in fields[1:]]
    frequencies = {}
    for p in Path("/sys/devices/system/cpu").glob("cpu[0-9]*/cpufreq/scaling_cur_freq"):
        try:
            frequencies[p.parent.parent.name] = int(p.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            pass
    return {"observed_at_unix_ns": time.time_ns(), "load_average": list(os.getloadavg()),
            "proc_loadavg": Path("/proc/loadavg").read_text(encoding="utf-8").strip(),
            "cpu_ticks": cpu, "cpu_tick_fields": ["user", "nice", "system", "idle", "iowait", "irq", "softirq", "steal", "guest", "guest_nice"],
            "cpu_frequency_khz": frequencies,
            "allowed_cpu_affinity": sorted(os.sched_getaffinity(0))}


def host_record():
    model = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines()
                  if line.startswith("model name")), None)
    cgroup = {}
    for name in ("cpu.max", "cpu.stat", "cpuset.cpus.effective", "memory.max"):
        p = Path("/sys/fs/cgroup") / name
        if p.is_file():
            cgroup[name] = p.read_text(encoding="utf-8").strip()
    cpuinfo = Path("/proc/cpuinfo").read_text(encoding="utf-8")
    features = next((line.split(":", 1)[1].strip().split() for line in cpuinfo.splitlines() if line.startswith("flags")), [])
    governors = {}
    for path in Path("/sys/devices/system/cpu").glob("cpu[0-9]*/cpufreq/scaling_governor"):
        try: governors[path.parent.parent.name] = path.read_text(encoding="utf-8").strip()
        except OSError: pass
    return {"platform": platform.platform(), "uname": list(platform.uname()),
            "cpu_model": model, "cpu_features": features, "cpu_governors": governors, "cpu_count": os.cpu_count(), "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"),
            "runner_environment": {k: os.environ[k] for k in ENV_NAMES if k in os.environ},
            "os_release": Path("/etc/os-release").read_text(encoding="utf-8"), "cgroup": cgroup,
            "snapshot": snapshot()}


class Meter:
    def __init__(self, executable, root):
        self.executable = Path(executable).resolve()
        self.root = Path(root).resolve()
        self.rows = []
        self.serial = 0
        self.affinity = sorted(os.sched_getaffinity(0))

    def run(self, label, argv, cwd, env, timeout=600):
        self.serial += 1
        directory = self.root / (f"{self.serial:04d}-" + label)
        directory.mkdir(parents=True)
        before = snapshot()
        outside = foreign_builds(proc_table())
        if outside:
            save(directory / "foreign-builds.json", outside)
            raise RuntimeError("foreign build active before sample; no process was stopped")
        command = [str(self.executable), str(directory / "usage.json"), str(directory / "stdout.bin"),
                   str(directory / "stderr.bin"), str(timeout), "--", *map(str, argv)]
        launch = time.perf_counter()
        observer_before = resource.getrusage(resource.RUSAGE_SELF)
        with (directory / "supervisor-stderr.bin").open("wb") as err:
            proc = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.DEVNULL, stderr=err)
            while proc.poll() is None:
                table = proc_table()
                outside.extend(foreign_builds(table, (proc.pid,)))
                time.sleep(.05)
            rc = proc.wait()
        outer_elapsed = time.perf_counter() - launch
        observer_after = resource.getrusage(resource.RUSAGE_SELF)
        after = snapshot()
        row = json.loads((directory / "usage.json").read_bytes())
        row.update(label=label, argv=list(map(str, argv)), cwd=str(Path(cwd).resolve()),
                   outer_observer_seconds=outer_elapsed, host_before=before, host_after=after,
                   observer_user_cpu_seconds=observer_after.ru_utime - observer_before.ru_utime,
                   observer_system_cpu_seconds=observer_after.ru_stime - observer_before.ru_stime,
                   environment={k: env[k] for k in sorted(env)
                                if k.startswith(("YAVO_", "ZIG_")) or k in ("LC_ALL", "LANG", "RUSTFLAGS", "CARGO_TARGET_DIR")},
                   foreign_builds=[dict(t) for t in {json.dumps(r, sort_keys=True): r for r in outside}.values()],
                   stdout=identity(directory / "stdout.bin"), stderr=identity(directory / "stderr.bin"))
        row["valid_isolation"] = not outside and before["allowed_cpu_affinity"] == after["allowed_cpu_affinity"] == self.affinity
        row["supervisor_exit_code"] = rc
        self.rows.append(row)
        save(directory / "record.json", row)
        if not row["valid_isolation"]:
            raise RuntimeError("sample contaminated by another build or affinity change; raw sample retained")
        if rc != row["exit_code"] or rc or row["timed_out"]:
            raise RuntimeError(f"command failed: {label}; raw stdout/stderr/usage retained")
        return row


def elf_record(path):
    data = Path(path).read_bytes()
    if data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("expected ELF64 little-endian input")
    h = struct.unpack_from("<16sHHIQQQIHHHHHH", data)
    if h[2] != 62 or h[11] != 64 or not h[12] or h[13] >= h[12]:
        raise ValueError("unsupported ELF architecture or extended section table")
    headers = [struct.unpack_from("<IIQQQQIIQQ", data, h[6] + i * h[11]) for i in range(h[12])]
    names = headers[h[13]]
    table = data[names[4]:names[4] + names[5]]
    def cstr(blob, offset):
        return blob[offset:blob.index(0, offset)].decode("utf-8", errors="replace")
    sections = []
    for i, s in enumerate(headers):
        file_bytes = 0 if s[1] == 8 else s[5]
        sections.append({"index": i, "name": cstr(table, s[0]), "type": s[1], "flags": s[2],
                         "address": s[3], "offset": s[4], "size": s[5], "file_bytes": file_bytes,
                         "sha256": hashlib.sha256(data[s[4]:s[4] + file_bytes]).hexdigest()})
    symbols = []
    for s in headers:
        if s[1] != 2:
            continue
        strings = headers[s[6]]
        blob = data[strings[4]:strings[4] + strings[5]]
        if s[9] != 24:
            raise ValueError("unexpected ELF64 symbol entry size")
        for offset in range(s[4], s[4] + s[5], s[9]):
            name, info, other, section, value, size = struct.unpack_from("<IBBHQQ", data, offset)
            if name and size and section and section < len(headers) and info & 15 in (1, 2):
                symbols.append({"name": cstr(blob, name), "kind": info & 15, "binding": info >> 4,
                                "section": sections[section]["name"], "address": value, "size": size})
    return {**identity(path), "elf_type": h[1], "machine": h[2], "sections": sections,
            "section_file_bytes": sum(s["file_bytes"] for s in sections), "symbols": symbols}


def mask_literals(source):
    out = list(source)
    i = 0
    while i < len(source):
        start = i
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = len(source) if end < 0 else end
        elif source.startswith("/*", i):
            depth = 1
            i += 2
            while i < len(source) and depth:
                if source.startswith("/*", i): depth += 1; i += 2
                elif source.startswith("*/", i): depth -= 1; i += 2
                else: i += 1
            if depth: raise ValueError("unterminated comment")
        elif source[i] in ('"', "'"):
            quote = source[i]
            i += 1
            while i < len(source):
                if source[i] == "\\": i += 2
                elif source[i] == quote: i += 1; break
                else: i += 1
            else: raise ValueError("unterminated literal")
        else:
            i += 1
            continue
        for at in range(start, min(i, len(source))):
            if out[at] != "\n": out[at] = " "
    return "".join(out)


def balanced_end(masked, start):
    pairs = {"(": ")", "[": "]", "{": "}"}
    stack = [pairs[masked[start]]]
    for at in range(start + 1, len(masked)):
        char = masked[at]
        if char in pairs: stack.append(pairs[char])
        elif char in ")]}":
            if not stack or char != stack.pop(): raise ValueError("unbalanced frozen declaration")
            if not stack: return at
    raise ValueError("unterminated frozen declaration")


def parameter_count(parameters):
    parameters = parameters.strip()
    if parameters in ("", "void"): return 0
    count, at = 1, 0
    while at < len(parameters):
        if parameters[at] in "([{": at = balanced_end(parameters, at)
        elif parameters[at] == ",": count += 1
        at += 1
    return count


def body_values(body, *, yavo=False):
    # Source and generated C use separate exact escape decoders. A function
    # containing unsupported strings contributes numeric anchors only, never a
    # guessed value. Long strings and large integers avoid source spans.
    try:
        _, strings = literal_octal_control(body.encode("utf-8"), collect_values=True, yavo_strings=yavo)
        values = {("string", value) for value in strings["decoded_values_hex"] if len(value) >= 24}
    except ValueError:
        values = set()
    for token in re.findall(r"(?<![\w.])(?:0[xX][0-9a-fA-F]+|0[bB][01]+|[0-9]+)(?:[uUlL]*)(?![\w.])", mask_literals(body)):
        token = token.rstrip("uUlL")
        value = int(token, 16 if token.startswith(("0x", "0X")) else 2 if token.startswith(("0b", "0B")) else 10)
        if value >= 1048576: values.add(("integer", str(value)))
    return values


def source_symbol_map(source_dir, generated):
    source_dir = Path(source_dir)
    keys = ["main"]
    functions, types, modules, source_bodies = [], [], [], []
    for key in keys:
        path = source_dir / (key + ".yv")
        text = path.read_text(encoding="utf-8")
        masked = mask_literals(text)
        depth, offset = 0, 0
        imports = []
        for line_no, line in enumerate(masked.splitlines(keepends=True), 1):
            if depth == 0:
                found = re.match(r"\s*(?:pub\s+)?fn\s+([A-Za-z_]\w*)\s*\(", line)
                if found:
                    paren = offset + found.end() - 1
                    end_params = balanced_end(masked, paren)
                    brace = masked.find("{", end_params + 1)
                    if brace < 0: raise ValueError("missing frozen function body")
                    end_body = balanced_end(masked, brace)
                    functions.append({"module": key, "name": found[1], "line": line_no,
                                      "parameter_count": parameter_count(masked[paren + 1:end_params])})
                    source_bodies.append(text[brace + 1:end_body])
                found = re.match(r"\s*(?:pub\s+)?(?:struct|enum)\s+([A-Za-z_]\w*)", line)
                if found: types.append({"module": key, "name": found[1], "line": line_no})
                found = re.match(r"\s*use\s+(.+?)\s*$", line)
                if found:
                    item = re.sub(r"\s+", "", found[1])
                    if not item.startswith("std/"):
                        group = re.fullmatch(r"([A-Za-z_][\w/]*/)?\{([\w,]+)\}", item)
                        if group: imports.extend((group[1] or "") + leaf for leaf in group[2].split(","))
                        elif re.fullmatch(r"[A-Za-z_][\w/]*", item): imports.append(item)
                        else: raise ValueError("unhandled frozen module import: " + item)
            depth += line.count("{") - line.count("}")
            offset += len(line)
            if depth < 0: raise ValueError("unbalanced frozen source")
        if depth: raise ValueError("unbalanced frozen source")
        for item in imports:
            if item not in keys: keys.append(item)
        modules.append({"key": key, **identity(path)})
    c_text = Path(generated).read_text(encoding="utf-8")
    c_masked = mask_literals(c_text)
    c_bodies, signatures, declarations = {}, {}, set()
    for found in re.finditer(r"(?m)^static\s+\w[\w *]*\s+sh_fn_([0-9]+)\(", c_masked):
        index = int(found[1]); declarations.add(index)
        paren = found.end() - 1
        end_params = balanced_end(c_masked, paren)
        brace = end_params + 1
        while brace < len(c_masked) and c_masked[brace].isspace(): brace += 1
        if brace == len(c_masked) or c_masked[brace] != "{": continue
        if index in c_bodies: raise ValueError("duplicate generated function definition")
        end_body = balanced_end(c_masked, brace)
        c_bodies[index] = c_text[brace + 1:end_body]
        signatures[index] = parameter_count(c_masked[paren + 1:end_params])
    expected = set(range(len(functions)))
    if declarations != expected or set(c_bodies) != expected:
        raise ValueError("source-order map does not match generated declarations and definitions")
    for index, function in enumerate(functions):
        if signatures[index] != function["parameter_count"]:
            raise ValueError("source/C parameter-count anchor differs: " + function["module"] + "." + function["name"])
    entry_indices = [i for i, f in enumerate(functions) if f["module"] == "main" and f["name"] == "main"]
    entries = list(re.finditer(r"(?m)^int\s+main\s*\(", c_masked))
    if len(entry_indices) != 1 or len(entries) != 1: raise ValueError("missing unique root entrypoint")
    entry_end = balanced_end(c_masked, entries[0].end() - 1)
    brace = c_masked.find("{", entry_end + 1)
    entry_body = c_masked[brace + 1:balanced_end(c_masked, brace)]
    calls = [int(n) for n in re.findall(r"\bsh_fn_([0-9]+)\s*\(", entry_body)]
    if calls != entry_indices: raise ValueError("generated entrypoint does not call mapped main.main")
    source_owners, c_owners = {}, {}
    for index, body in enumerate(source_bodies):
        for value in body_values(body, yavo=True): source_owners.setdefault(value, set()).add(index)
    for index, body in c_bodies.items():
        for value in body_values(body): c_owners.setdefault(value, set()).add(index)
    anchors = []
    for value in sorted(source_owners):
        owners = source_owners[value]
        if len(owners) != 1 or len(c_owners.get(value, ())) != 1: continue
        if owners != c_owners[value]:
            raise ValueError("source/C unique body anchor identifies a different function")
        index = next(iter(owners)); function = functions[index]
        if value[0] == "string":
            data = bytes.fromhex(value[1])
            evidence = {"kind": "decoded_string", "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        else: evidence = {"kind": "integer", "value": value[1]}
        anchors.append({"module": function["module"], "function": function["name"], "index": index, **evidence})
    required_modules = {f["module"] for f in functions}
    anchored_modules = {a["module"] for a in anchors}
    if anchored_modules != required_modules:
        raise ValueError("no independent generated-body anchor for modules: " + ",".join(sorted(required_modules - anchored_modules)))
    # At most two strongest independent values per module keep the evidence small.
    selected = []
    for module in keys:
        candidates = [a for a in anchors if a["module"] == module]
        selected.extend(sorted(candidates, key=lambda a: (a["kind"] != "decoded_string", -a.get("bytes", 0), a["index"]))[:2])
    return {"loader_order": "main.compile breadth-first imports; parser.parseFile appends source-order functions",
            "modules": modules, "functions": {str(i): row for i, row in enumerate(functions)},
            "types": {str(i): row for i, row in enumerate(types)},
            "independent_c_validation": {"definition_count": len(c_bodies), "parameter_counts_checked": len(functions),
                "entrypoint_calls": calls, "unique_body_anchors_checked": len(anchors),
                "anchored_modules": sorted(anchored_modules), "selected_body_anchors": selected}}



def symbol_sizes(elf, names):
    sizes, seen = {}, set()
    for symbol in sorted(elf["symbols"], key=lambda x: (not x["name"].startswith("sh_fn_"), x["name"])):
        if symbol["section"] != ".text": continue
        region = (symbol["section"], symbol["address"], symbol["size"])
        if region in seen: continue
        seen.add(region)
        name = symbol["name"]
        match = re.fullmatch(r"sh_fn_([0-9]+)(.*)", name)
        if match:
            source = names["functions"][match[1]]
            name = source["module"] + "." + source["name"] + match[2]
        elif name.startswith("sh_"):
            name = "generated-helper:" + re.sub(r"[0-9]+", "#", name)
        sizes[name] = sizes.get(name, 0) + symbol["size"]
    return sizes


def literal_octal_control(data, *, collect_values=False, yavo_strings=False):
    """Rewrite ordinary C string tokens only, preserving their decoded bytes.

    Preprocessor directives, char constants and comments are untouched. Narrow
    generated strings use only standard byte escapes; unsupported prefixes and
    universal-character escapes fail instead of silently changing semantics.
    The separate yavo_strings mode is used only for source identity anchors;
    it decodes Yavo escapes and rejects interpolation, never the C control.
    """
    escapes = {ord(k): v for k, v in {'a':7, 'b':8, 'f':12, 'n':10, 'r':13, 't':9, 'v':11,
                                     '\\':92, '"':34, "'":39, '?':63}.items()}
    out = bytearray(); i = 0; decoded = hashlib.sha256(); count = 0; values = []
    while i < len(data):
        start = i
        if (i == 0 or data[i - 1] == 10) and data[i:i+1] in (b'#', b' ', b'\t'):
            eol = data.find(b'\n', i)
            eol = len(data) if eol < 0 else eol + 1
            if data[i:eol].lstrip().startswith(b'#'):
                out.extend(data[i:eol]); i = eol; continue
        if data.startswith(b'//', i):
            end = data.find(b'\n', i); i = len(data) if end < 0 else end
            out.extend(data[start:i]); continue
        if data.startswith(b'/*', i):
            end = data.find(b'*/', i + 2)
            if end < 0: raise ValueError('unterminated C comment')
            i = end + 2; out.extend(data[start:i]); continue
        if data[i] == 39:
            i += 1
            while i < len(data):
                if data[i] == 92: i += 2
                elif data[i] == 39: i += 1; break
                else: i += 1
            out.extend(data[start:i]); continue
        if data[i] != 34:
            out.append(data[i]); i += 1; continue
        if i and (chr(data[i-1]).isalnum() or data[i-1] == 95):
            raise ValueError('prefixed C string outside byte-control contract')
        i += 1; value = bytearray()
        while i < len(data) and data[i] != 34:
            byte = data[i]; i += 1
            if byte != 92:
                if yavo_strings and byte == 36 and data[i:i+1] == b"{":
                    raise ValueError("interpolated Yavo string is not a literal anchor")
                if byte in (10, 13): raise ValueError('raw newline in C string')
                value.append(byte); continue
            if i == len(data): raise ValueError('truncated C escape')
            byte = data[i]; i += 1
            if yavo_strings:
                yavo_escapes = {110: 10, 116: 9, 114: 13, 48: 0, 92: 92, 34: 34, 39: 39, 36: 36}
                if byte in yavo_escapes: value.append(yavo_escapes[byte])
                elif byte == 117 and data[i:i+1] == b"{":
                    end = data.find(b"}", i + 1)
                    digits = data[i+1:end] if end >= 0 else b""
                    if not (1 <= len(digits) <= 6) or any(c not in b"0123456789abcdefABCDEF" for c in digits):
                        raise ValueError("invalid Yavo unicode escape")
                    codepoint = int(digits, 16)
                    if codepoint > 0x10ffff or 0xd800 <= codepoint <= 0xdfff: raise ValueError("invalid Yavo scalar")
                    value.extend(chr(codepoint).encode("utf-8")); i = end + 1
                else: raise ValueError("unsupported Yavo escape")
            elif byte in escapes: value.append(escapes[byte])
            elif 48 <= byte <= 55:
                digits = bytes([byte])
                while len(digits) < 3 and i < len(data) and 48 <= data[i] <= 55:
                    digits += bytes([data[i]]); i += 1
                number = int(digits, 8)
                if number > 255: raise ValueError('non-byte octal escape')
                value.append(number)
            elif byte == 120:
                digits = bytearray()
                while i < len(data) and data[i] in b'0123456789abcdefABCDEF':
                    digits.append(data[i]); i += 1
                if not digits or int(digits, 16) > 255: raise ValueError('non-byte hex escape')
                value.append(int(digits, 16))
            elif byte == 10: pass
            else: raise ValueError('unsupported C escape')
        if i >= len(data): raise ValueError('unterminated C string')
        i += 1
        decoded.update(struct.pack('<Q', len(value))); decoded.update(value); count += 1
        if collect_values: values.append(bytes(value).hex())
        out.extend(b'"' + b''.join(('\\%03o' % b).encode('ascii') for b in value) + b'"')
    result = {"string_token_count": count, "decoded_string_sequence_sha256": decoded.hexdigest()}
    if collect_values: result["decoded_values_hex"] = values
    return bytes(out), result
