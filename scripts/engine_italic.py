import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from engine_core import run_batch

if __name__ == "__main__":
    run_batch(sys.argv[1:], style="italic", default_mode="visual")
