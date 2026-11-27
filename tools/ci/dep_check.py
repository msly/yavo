#!/usr/bin/env python3
"""Native dependency-index acceptance using isolated configured app runners."""
import sys

sys.dont_write_bytecode = True
import dep_support


def main():
    return dep_support.create_native().main()


if __name__ == "__main__":
    raise SystemExit(main())
