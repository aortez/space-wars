"""Create image-validation policy from an independently approved source checkout.

Run this trusted helper without candidate code or signing credentials. The caller
must first admit the exact source through GitHub/CI policy. This does not choose
that source, grant approval, or read a candidate's self-reported qualification.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

import yaml

from ota_build_inputs import CONFIG, blob, clean_repository, expected_inputs


def from_source(repo, source):
    repo = repo.resolve()
    clean_repository(repo, source)
    path = repo / 'yocto' / CONFIG
    selected = blob(path)
    raw = path.read_bytes()
    if len(raw) != selected['bytes'] or hashlib.sha256(raw).hexdigest() != selected['sha256']:
        raise ValueError('Selected source configuration changed')
    configuration = yaml.safe_load(raw)
    # No includes, lock/override documents, env expansion or source-provided
    # commands are evaluated by the publisher. kas dump of this one file must
    # produce exactly the same canonical configuration as the source parser.
    if (not isinstance(configuration, dict)
            or set(configuration) != {'header', 'distro', 'machine', 'env', 'repos', 'local_conf_header', 'target'}
            or configuration['header'] != {'version': 14}
            or configuration['env'] != {'SPACEWARS_CACHE_ROOT': None,
                                       'SPACEWARS_OTA_BUILD_INPUTS': None,
                                       'SPACEWARS_OTA_BUILD_INPUTS_SHA256': None}):
        raise ValueError('Source configuration requires a policy review')
    expected = expected_inputs(repo, source, configuration)
    if expected['configuration']['yocto/kas-spacewars.yml'] != selected:
        raise ValueError('Selected configuration changed during policy creation')
    raw_inputs = json.dumps(expected, sort_keys=True, separators=(',', ':')).encode()
    clean_repository(repo, source)
    # These are publisher policy, not values copied from compiler claims. A
    # compiler update needs a reviewed change here as well as in the recipe.
    return dict(schema=1, source_commit=source,
                build_inputs_sha256=hashlib.sha256(raw_inputs).hexdigest(),
                rust_version='1.94.0', cargo_version='1.94.0',
                cc_version_line='aarch64-poky-linux-gcc (GCC) 13.5.0')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-checkout', type=Path, required=True)
    parser.add_argument('--approved-source', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    policy = from_source(args.source_checkout, args.approved_source)
    with args.output.open('x') as stream:
        json.dump(policy, stream, sort_keys=True, indent=2)
        stream.write('\n')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, yaml.YAMLError) as error:
        sys.exit(str(error))
