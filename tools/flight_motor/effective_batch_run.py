"""LF-073 retained training and exact resumption entry point."""
from . import effective_batch
from .movement_run import main

if __name__ == '__main__':
    main(effective_batch)
