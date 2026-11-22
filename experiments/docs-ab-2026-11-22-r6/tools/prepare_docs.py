#!/usr/bin/env python3
"""Snapshot canonical docs and deterministically derive equal-access API topics."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tempfile

BASE = Path(__file__).resolve().parents[1]
REPO = BASE.parents[1]
SOURCE_NAMES = ("cheatsheet.md", "agent-start.md", "agent-api-index.md",
                "semantics.md", "decisions.md", "stdlib_contract.md")
TOPICS = {
    "syntax": [("cheatsheet.md", 5, 39), ("semantics.md", 136, 187)],
    "types": [("cheatsheet.md", 40, 133), ("semantics.md", 27, 85)],
    "errors": [("cheatsheet.md", 134, 200), ("semantics.md", 44, 55), ("semantics.md", 127, 133)],
    "ownership": [("cheatsheet.md", 201, 290), ("semantics.md", 7, 115)],
    "order": [("semantics.md", 116, 135), ("decisions.md", 72, 91)],
    "io": [("cheatsheet.md", 291, 300), ("cheatsheet.md", 318, 361), ("stdlib_contract.md", 77, 105)],
    "strings": [("cheatsheet.md", 301, 307), ("stdlib_contract.md", 77, 105)],
    "vec": [("cheatsheet.md", 308, 309), ("cheatsheet.md", 312, 312), ("stdlib_contract.md", 17, 32), ("stdlib_contract.md", 61, 76)],
    "map-set": [("cheatsheet.md", 310, 312), ("cheatsheet.md", 314, 314), ("stdlib_contract.md", 33, 60)],
    "heap-deque": [("cheatsheet.md", 313, 314)],
    "arena": [("cheatsheet.md", 267, 289), ("cheatsheet.md", 315, 315), ("semantics.md", 136, 167)],
    "math": [("cheatsheet.md", 316, 316)],
    "drop": [("cheatsheet.md", 363, 375), ("semantics.md", 116, 135), ("stdlib_contract.md", 106, 111)],
    "modules": [("cheatsheet.md", 378, 409)],
    "threads": [("cheatsheet.md", 410, 444), ("semantics.md", 94, 115), ("stdlib_contract.md", 112, 141)],
    "tools": [("cheatsheet.md", 445, 460), ("stdlib_contract.md", 142, 148)],
    "diagnostics": [("cheatsheet.md", 461, 494)],
    "unsupported": [("cheatsheet.md", 511, 513)],
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def save(root, name, data):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def encode_json(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()


def pages(raw, limit=8192):
    result, page = [], bytearray()
    for char in raw.decode("utf-8"):
        encoded = char.encode()
        if len(page) + len(encoded) > limit:
            result.append(bytes(page))
            page.clear()
        page.extend(encoded)
    if page or not result:
        result.append(bytes(page))
    return result


def derive(sources, output):
    raw = {name: (sources / name).read_bytes() for name in SOURCE_NAMES}
    text = {name: data.decode("utf-8").splitlines(keepends=True) for name, data in raw.items()}
    # Both arms receive the identical index; remove the general guide's instruction
    # to read the compact card so the full arm is not redirected to the treatment.
    index = raw["agent-api-index.md"].decode("utf-8").replace(
        "先读 [agent-start.md](agent-start.md)", "先读本会话分配的 `guide`", 1)
    index += "\n实验读取：`doc guide PAGE`、`doc index PAGE`、`doc api.TOPIC PAGE`。PAGE 从 1 开始，每页至多 8192 个 UTF-8 字节；先读完 guide 和 index 的全部页，再写题目。两个文档组可访问相同的十八个 API 主题。\n"
    save(output, "docs/api-index.md", index.encode())
    documents = {
        "guide": {"path": {"full": "docs/sources/cheatsheet.md", "compact": "docs/sources/agent-start.md"}, "category": "read_initial_guide"},
        "index": {"path": "docs/api-index.md", "category": "read_api_index"},
        "instructions": {"path": "PARTICIPANT.md", "category": "read_instructions"},
    }
    for task in range(1, 9):
        name = f"T{task:02d}"
        documents[name] = {"path": f"public/{name}.md", "category": "read_task"}
    for topic, ranges in TOPICS.items():
        body = [f"# API 主题：{topic}\n\n", "当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。\n"]
        for name, start, end in ranges:
            if not 1 <= start <= end <= len(text[name]):
                raise ValueError(f"invalid source range: {name}:{start}-{end}")
            body.extend([f"\n## {name}，第 {start}–{end} 行\n\n", "".join(text[name][start-1:end])])
        path = f"docs/api/{topic}.md"
        save(output, path, "".join(body).encode())
        documents["api." + topic] = {"path": path, "category": "read_api_topic"}
    save(output, "documents.json", encode_json({"schema_version": 1, "page_bytes": 8192, "documents": documents}))
    metrics = {
        "schema_version": 1,
        "source_files": {name: {"sha256": digest(data), "bytes": len(data)} for name, data in raw.items()},
        "shared_index": {"sha256": digest(index.encode()), "bytes": len(index.encode()), "pages": len(pages(index.encode()))},
        "arms": {arm: {"guide_bytes": len(raw[name]), "guide_sha256": digest(raw[name]), "guide_pages": len(pages(raw[name])),
                       "mandatory_document_bytes": len(raw[name]) + len(index.encode()), "mandatory_document_operations": len(pages(raw[name])) + len(pages(index.encode()))}
                 for arm, name in (("full", "cheatsheet.md"), ("compact", "agent-start.md"))},
        "index_transform": "Replace starter-only first-read instruction with assigned guide; append identical lab topic/page instructions for both arms.",
        "topics": TOPICS,
        "measurement": "UTF-8 payload bytes and deterministic page counts; these are not provider input tokens or measured participant retrieval counts.",
    }
    save(output, "docs/document-manifest.json", encode_json(metrics))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--snapshot-from-repo", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    sources = BASE / "docs/sources"
    if args.snapshot_from_repo:
        if (BASE / "evidence/frozen.json").exists() or (BASE / "runs").exists():
            parser.error("cannot replace document snapshots after runtime freeze or participant initialization")
        sources.mkdir(parents=True, exist_ok=True)
        for name in SOURCE_NAMES:
            shutil.copyfile(REPO / "docs" / name, sources / name)
    if args.check:
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)
            derive(sources, destination)
            generated = [p for p in destination.rglob("*") if p.is_file()]
            for path in generated:
                relative = path.relative_to(destination)
                if path.read_bytes() != (BASE / relative).read_bytes():
                    raise ValueError("derived document changed: " + str(relative))
            print(json.dumps({"status": "passed", "generated_files": len(generated), "topics": len(TOPICS)}))
    else:
        derive(sources, BASE)


if __name__ == "__main__":
    main()
