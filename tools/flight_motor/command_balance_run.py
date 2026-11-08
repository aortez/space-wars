"""LF-067 command-balanced collection through the protected runner."""
from . import command_balance
from .movement_run import main

if __name__ == '__main__':
    main(command_balance)
