"""LF-065 uses the shared protected runner without changing its default stage."""
from .movement_run import main
from . import gae_comparison

if __name__ == '__main__':
    main(gae_comparison)
