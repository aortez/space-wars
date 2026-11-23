"""Decision checks of ota_release.py with fake signatures; ota_release_fixture.py runs the real tools."""
from datetime import timedelta
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import ota_prepare
import ota_publisher_journal as journal
import ota_release as release
from ota_delivery import digest
from test_ota_publisher_journal import NOW, ROOT, FakeTooling, finish, genesis, nightly, staged

EXPIRES = '2026-11-27T07:58:33Z'
CI_URL = 'https://github.com/aortez/space-wars/actions/runs/7'


def configuration(root):
    return dict(schema=1, authority=str(root / 'authority'), workspace=str(root / 'workspace'),
                root_sha256=digest(ROOT), tools_directory=str(root / 'tools'),
                tools={name: 'f' * 64 for name in release.TOOLS}, code={name: 'c' * 64 for name in release.CODE},
                build=dict(cache=str(root / 'cache'), checkout=str(root / 'checkout'), directory=str(root / 'build')))


def publisher(root):
    (root / 'workspace').mkdir()
    return release.Publisher(release.Config(configuration(root)))


def prepared_candidate(directory, payload, embedded='a' * 40, baseline=None, code=None):
    """A candidate directory as ota_release.py candidate leaves it, from source 'a' * 40, compared with the
    baseline generation by the code configuration() pins, unless code names other, and its image_candidate
    output."""
    (directory / 'candidate').mkdir(parents=True)
    descriptor = dict(schema=1, format='rootfs-ext4-zstd', expanded=dict(bytes=4096, sha256=digest(payload * 2)),
                      compressed=dict(bytes=len(payload), sha256=digest(payload)))
    image = dict(schema=1, source=dict(schema=1, commit=embedded, dirty=False, revision=embedded[:12]),
                 build_id='fixture-rootfs', machine='raspberrypi-spacewars', architecture='aarch64',
                 layout='spacewars-ab-v1', boot_identity='b' * 64, data=dict(reads=dict(min=1, max=1), writes=1),
                 updater_protocol=1, recovery_protocol=2, binaries={}, rootfs_tree='t' * 64)
    files = {'policy.json': b'{}', 'candidate/image.ext4.zst': payload,
             'main-ci-run.json': json.dumps(dict(url=CI_URL)).encode(),
             'candidate/candidate.json': json.dumps(descriptor).encode(),
             'candidate/qualification.json': json.dumps(dict(schema=1, image=image)).encode()}
    for name, data in files.items():
        (directory / name).write_bytes(data)
    code = {name: 'c' * 64 for name in release.CODE} if code is None else code
    report = dict(schema=2, prepared_by=dict(command='ota_release.py candidate', code=code), source='a' * 40,
                  descriptor=descriptor, baseline=baseline and dict(generation=baseline),
                  artifacts={name: dict(bytes=len(data), sha256=digest(data)) for name, data in files.items()})
    (directory / 'preparation-result.json').write_text(json.dumps(report))
    return files['candidate/qualification.json']


def target(image, channel, generation):
    return dict(length=image['compressed']['bytes'], hashes=dict(sha256=image['compressed']['sha256']),
                custom=dict(spacewars=dict(image, channel=channel, generation=generation)))


def dated_feed():
    """A genesis journal whose roles carry an expiry, and the manifest standing in for its bundle."""
    roles = {r: journal.encode(dict(signed=dict(_type=r, version=1, expires=EXPIRES, targets={}),
                                    signatures=[])).decode() for r in journal.ROLES}
    manifest = journal.encode(roles)
    raw = roles['timestamp'].encode()
    record = dict(manifest_sha256=digest(manifest), version=1, timestamp=dict(bytes=len(raw), sha256=digest(raw)))
    return journal.initialize(ROOT, roles, record), manifest


class TargetTests(unittest.TestCase):
    def setUp(self):
        self.first, self.second = nightly(b'first')['image'], nightly(b'second')['image']
        self.old = dict(release={}, nightly={'nightly.' + self.first['compressed']['sha256'] + '.ext4.zst':
                                             target(self.first, 'nightly', 1)})

    def test_renewal_keeps_every_target(self):
        release.check_targets(self.old, self.old, release.RENEW, None)
        with self.assertRaisesRegex(ValueError, 'renewal changed targets'):
            release.check_targets(self.old, dict(self.old, release=self.old['nightly']), release.RENEW, None)

    def test_nightly_gains_exactly_the_selected_image(self):
        added = {'nightly.' + self.second['compressed']['sha256'] + '.ext4.zst': target(self.second, 'nightly', 2)}
        intent = dict(kind='nightly', image=self.second)
        release.check_targets(self.old, dict(self.old, nightly={**self.old['nightly'], **added}), intent, 2)
        first = next(iter(self.old['nightly']))
        wrong = {'another generation': {**self.old['nightly'], **added},
                 'an existing target changed': {first: target(self.first, 'nightly', 3), **added},
                 'an extra target': {**self.old['nightly'], **added,
                                     'nightly.x.ext4.zst': target(self.first, 'nightly', 1)},
                 'nothing added': self.old['nightly']}
        for case, value in wrong.items():
            with self.subTest(case), self.assertRaises(ValueError):
                release.check_targets(self.old, dict(self.old, nightly=value), intent,
                                      3 if case == 'another generation' else 2)

    def test_release_gains_the_nightly_target_relabelled(self):
        payload = self.first['compressed']['sha256']
        intent = dict(kind='release', payload_sha256=payload, notes='n', pi_qualification_sha256='d' * 64)
        promoted = {'release.' + payload + '.ext4.zst': target(self.first, 'release', 1)}
        release.check_targets(self.old, dict(self.old, release=promoted), intent, 1)
        for case, new in (('another generation', dict(self.old, release={k: target(self.first, 'release', 2)
                                                                           for k in promoted})),
                          ('Nightly changed', dict(release=promoted, nightly={}))):
            with self.subTest(case), self.assertRaises(ValueError):
                release.check_targets(self.old, new, intent, 1)
        with self.assertRaisesRegex(ValueError, 'not a Nightly target'):
            release.check_targets(dict(self.old, nightly={}), dict(release=promoted, nightly={}), intent, 1)


class PhaseTests(unittest.TestCase):
    def setUp(self):
        self.base = genesis()
        self.plan = dict(version=2, intent=nightly())

    def test_phases_follow_the_planned_operation(self):
        pending = journal.reserve(self.base, self.plan['intent'], NOW)
        self.assertEqual(release.phase(self.base, self.base, self.plan), 'initial')
        self.assertEqual(release.phase(pending, self.base, self.plan), 'pending')
        self.assertEqual(release.phase(finish(pending, FakeTooling()), self.base, self.plan), 'complete')

    def test_any_other_change_rejects_the_plan(self):
        renewal = journal.reserve(self.base, release.RENEW, NOW)
        with self.assertRaisesRegex(ValueError, 'Another publisher operation is pending'):
            release.phase(renewal, self.base, self.plan)
        with self.assertRaisesRegex(ValueError, 'changed since this operation was prepared'):
            release.phase(finish(renewal, FakeTooling()), self.base, self.plan)

    def test_pending_operation_resumes_only_with_a_day_left(self):
        pending = dict(intent=release.RENEW, prepared_at=NOW)
        release.check_resumable(pending, release.parse(NOW) + timedelta(days=5, hours=23))
        with self.assertRaisesRegex(ValueError, 'too old to publish; abandon it first'):
            release.check_resumable(pending, release.parse(NOW) + timedelta(days=6))

    def test_unsigned_roles_are_signed_only_within_the_signers_window(self):
        reserved = journal.reserve(self.base, release.RENEW, NOW)
        start = release.parse(NOW)
        release.check_signable(reserved['pending'], start + timedelta(hours=23))
        with self.assertRaisesRegex(ValueError, 'signer accepts them only for 24 hours'):
            release.check_signable(reserved['pending'], start + timedelta(hours=24))
        release.check_signable(staged(reserved, FakeTooling())['pending'], start + timedelta(days=3))


class ConfigTests(unittest.TestCase):
    def test_configuration_is_complete_and_private(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            value = configuration(root)
            for case, changed in (('a missing tool', dict(value, tools={'feed_sign': 'f' * 64})),
                                  ('missing code pins', dict(value, code={})),
                                  ('no build paths', dict(value, build={})),
                                  ('a relative build path', dict(value, build=dict(value['build'], cache='cache'))),
                                  ('a relative path', dict(value, workspace='workspace')),
                                  ('an unknown field', dict(value, extra=1))):
                with self.subTest(case), self.assertRaises(ValueError):
                    release.Config(changed)
            path = root / 'release.json'
            path.write_text(json.dumps(value))
            os.chmod(path, 0o600)
            self.assertEqual(release.Config.load(path).lock_path, root / 'publication.lock')
            os.chmod(path, 0o620)
            with self.assertRaisesRegex(ValueError, 'others cannot write'):
                release.Config.load(path)


class CandidateTests(unittest.TestCase):
    def test_preparation_runs_without_the_lock_under_the_owners_umask(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        config = release.Config(configuration(Path(temporary.name)))
        session = release.Publisher(config, exclusive=False)
        session.umask = 0o002  # What __enter__ saved before it set the session's private umask.
        seen = []

        def prepare(publisher, code, source, accept, generation, previous):
            seen.append(os.umask(0o022))
            os.umask(seen[-1])
            return dict(prepared=True)

        private = os.umask(0o077)
        try:
            with patch.object(ota_prepare, 'prepare', prepare):
                self.assertEqual(session.candidate(), dict(prepared=True))
            self.assertEqual((seen, os.umask(0o077)), ([0o002], 0o077))
        finally:
            os.umask(private)
        with self.assertRaisesRegex(ValueError, 'without the publisher lock'):
            release.Publisher(config).candidate()


class SelectionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.publisher = publisher(self.root)

    def candidate(self, embedded='a' * 40):
        """A prepared candidate, verified by a fake image_candidate and passed by a fake CI run on main."""
        directory = self.root / 'candidate'
        qualification = prepared_candidate(directory, b'compressed image', embedded)
        self.publisher.state = genesis()
        self.publisher.run = lambda name, *args, timeout=120: qualification
        self.ci = dict(status='completed', conclusion='success', event='push', headBranch='main', headSha='a' * 40,
                       workflowName='CI')
        self.publisher.gh_json = lambda args: dict(self.ci, url=args[2])
        return directory, b'compressed image'

    def test_nightly_selection_checks_the_candidate_and_ci_on_main(self):
        directory, payload = self.candidate()
        intent, selection, image = self.publisher.select_nightly(directory)
        self.assertEqual(journal.identity(intent['image']), digest(payload))
        self.assertEqual((intent['image']['source_commit'], intent['image']['min_recovery_protocol']), ('a' * 40, 2))
        self.assertEqual((selection['ci']['url'], image), ('7', directory / 'candidate/image.ext4.zst'))
        for case, field, value in (('another branch', 'headBranch', 'feature'), ('another commit', 'headSha', 'b' * 40),
                                   ('a failed run', 'conclusion', 'failure'), ('another workflow', 'workflowName',
                                                                             'Clock mixed-event regression')):
            with self.subTest(case), self.assertRaisesRegex(ValueError, 'has not passed CI on main'):
                self.ci[field] = value
                self.publisher.select_nightly(directory)
            self.ci = dict(self.ci, headBranch='main', headSha='a' * 40, conclusion='success', workflowName='CI')
        (directory / 'candidate/candidate.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'Preparation artifact changed'):
            self.publisher.select_nightly(directory)

    def test_nightly_selection_requires_a_candidate_the_command_compared_with_the_newest_generation(self):
        directory, _ = self.candidate()
        path = directory / 'preparation-result.json'
        report = json.loads(path.read_bytes())
        path.write_text(json.dumps(dict(report, schema=1)))
        with self.assertRaisesRegex(ValueError, 'Prepare candidates with ota_release.py candidate'):
            self.publisher.select_nightly(directory)
        path.write_text(json.dumps(dict(report, prepared_by=dict(report['prepared_by'], code={}))))
        with self.assertRaisesRegex(ValueError, 'was prepared by other code; delete it and run '
                                                'ota_release.py candidate a{40} again'):
            self.publisher.select_nightly(directory)
        path.write_text(json.dumps(report))
        self.publisher.state = finish(journal.reserve(genesis(), nightly(), NOW), FakeTooling())
        with self.assertRaisesRegex(ValueError, 'compared with generation None, but the newest published '
                                                'generation is 1; run ota_release.py candidate a{40} again'):
            self.publisher.select_nightly(directory)
        path.write_text(json.dumps(dict(report, baseline=dict(generation=1))))
        self.assertEqual(self.publisher.select_nightly(directory)[0]['image']['source_commit'], 'a' * 40)

    def test_nightly_selection_requires_the_embedded_source(self):
        directory, _ = self.candidate(embedded='c' * 40)
        with self.assertRaisesRegex(ValueError, 'Wrong embedded source'):
            self.publisher.select_nightly(directory)

    def test_release_selection_requires_the_record_of_the_published_image(self):
        selected = nightly()
        self.publisher.state = finish(journal.reserve(genesis(), selected, NOW), FakeTooling())
        payload = journal.identity(selected['image'])
        record = self.root / 'record.json'
        good = dict(purpose='release-qualification', image=dict(selected['image'], generation=1), pi_evidence={})
        record.write_text(json.dumps(good))
        intent, chosen, evidence = self.publisher.select_release(payload, record, 'Promote it.')
        self.assertEqual(evidence, record.read_bytes())
        self.assertEqual(intent['pi_qualification_sha256'], digest(evidence))
        self.assertEqual(chosen['nightly_publication'], self.publisher.state['images'][payload]['nightly'])
        for case, bad in (('another purpose', dict(good, purpose='nightly-trial')),
                          ('another generation', dict(good, image=dict(good['image'], generation=2))),
                          ('another build', dict(good, image=dict(good['image'], build_id='other')))):
            record.write_text(json.dumps(bad))
            with self.subTest(case), self.assertRaisesRegex(ValueError, 'names another image'):
                self.publisher.select_release(payload, record, 'Promote it.')
        with self.assertRaisesRegex(ValueError, 'Only a published Nightly image'):
            self.publisher.select_release('d' * 64, record, 'Promote it.')


class PreviousBundleTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.publisher = publisher(self.root)
        self.publisher.state, self.manifest = dated_feed()
        self.checked = []

    def inventory(self, name, root, path, at):
        self.checked.append((Path(path), at))
        return self.manifest

    def test_expired_feed_bundle_is_checked_at_its_last_valid_second(self):
        bundle = self.root / 'bundle'
        bundle.mkdir()
        self.publisher.run = self.inventory
        expires = release.parse(EXPIRES)
        for clock, checked in ((expires - timedelta(days=1), expires - timedelta(days=1)),
                               (expires + timedelta(days=20), expires - timedelta(seconds=1))):
            with self.subTest(clock=clock):
                self.publisher.now = lambda: clock
                self.assertEqual(self.publisher.previous_bundle(bundle), (bundle, self.manifest))
                self.assertEqual(self.checked[-1], (bundle, release.stamp(checked)))

    def test_a_relative_bundle_path_is_kept_absolute(self):
        bundle = self.root / 'bundle'
        bundle.mkdir()
        self.publisher.run = self.inventory
        self.publisher.now = lambda: release.parse(EXPIRES) - timedelta(days=1)
        self.assertEqual(self.publisher.previous_bundle(Path(os.path.relpath(bundle)))[0], bundle.resolve())

    def test_bundle_search_passes_over_unusable_candidates(self):
        config = self.publisher.config
        broken, good = config.operations / 'v1-renew/bundle', self.root / 'good'
        broken.mkdir(parents=True)
        good.mkdir()
        config.pointer.parent.mkdir()
        config.pointer.write_text(json.dumps(dict(bundle=str(good))))

        def inventory(name, root, path, at):
            if Path(path) == broken:
                raise release.Refused('feed_inventory refused: Error: ExpiredMetadata')
            return self.inventory(name, root, path, at)
        self.publisher.run = inventory
        self.assertEqual(self.publisher.previous_bundle()[0], good)
        self.manifest = b'{}'
        with self.assertRaisesRegex(ValueError, 'bundle: feed_inventory refused: Error: ExpiredMetadata; '
                                                '.*good: another feed version'):
            self.publisher.previous_bundle()


if __name__ == '__main__':
    unittest.main()
