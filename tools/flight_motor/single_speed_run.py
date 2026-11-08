"""LF-069 uses the unchanged protected training runner."""
from . import single_speed
from .movement_run import main

if __name__ == '__main__':
    main(single_speed)
