"""Journal state tests use fake signatures; feed_sign tests use real TUF keys."""
import copy
import json
import unittest

import ota_publisher_journal as helper
from ota_delivery import digest

NOW = '2026-10-17T03:00:00Z'
LATER = '2026-10-18T03:00:00Z'


def role_bytes(role, version):
    return helper.encode(dict(signed=dict(_type=role, version=version, targets={}), signatures=[]))


def record(roles):
    raw = roles['timestamp'].encode()
    return dict(manifest_sha256=digest(helper.encode(roles)), version=helper.metadata(roles['timestamp'])['version'],
                timestamp=dict(bytes=len(raw), sha256=digest(raw)))


def genesis():
    roles = {r: role_bytes(r, 1).decode() for r in helper.ROLES}
    return helper.initialize(b'independently verified fixture root', roles, record(roles))


def nightly(payload=b'first image'):
    image = dict(schema=1, source_commit='a' * 40, build_id='fixture-rootfs', machine='raspberrypi-spacewars',
                 architecture='aarch64', layout='spacewars-ab-v1', format='rootfs-ext4-zstd',
                 compressed=dict(bytes=len(payload), sha256=digest(payload)),
                 expanded=dict(bytes=4096, sha256=digest(payload * 2)), boot_identity='b' * 64,
                 min_updater_protocol=1, min_recovery_protocol=2, data=dict(reads=dict(min=1, max=1), writes=1))
    return dict(kind='nightly', image=image, evidence_sha256='c' * 64)


class FakeTooling:
    def __init__(self):
        self.calls = []

    def preview(self, root, authority, request):
        self.calls.append(('preview', root, authority, request))
        return helper.encode(dict(fixture_request_sha256=digest(request), fixture_authority_sha256=digest(authority.encode())))

    def verify(self, root, authority, request, approved, response):
        self.calls.append(('verify', root, authority, request, approved, response))
        expected = digest(helper.encode(dict(fixture_request_sha256=digest(request),
                                            fixture_authority_sha256=digest(authority.encode()))))
        if approved != expected or helper.metadata(response.decode())['version'] != json.loads(request)['version']:
            raise ValueError('Fixture response mismatch')


def finish(state, tooling):
    for role in helper.ROLES:
        state = helper.prepare(state, role, tooling)
        state = helper.retain(state, role, role_bytes(role, state['versions'][role]), tooling)
    roles = {r: helper.signed(state, r) for r in helper.ROLES}
    published = record(roles)
    return helper.confirm(state, published, published, state['pending']['prepared_at'])


class PublisherJournalTests(unittest.TestCase):
    def setUp(self):
        self.state = genesis()
        self.tooling = FakeTooling()

    def test_interrupted_reservation_resumes_time_versions_generation_and_exact_stages(self):
        state = helper.reserve(self.state, nightly(), NOW)
        for role in helper.ROLES:
            state = helper.prepare(state, role, self.tooling)
            state = helper.retain(state, role, role_bytes(role, 2), self.tooling)
            checkpoint = helper.encode(state)
            recovered = helper.load(checkpoint)
            self.assertEqual(helper.encode(helper.reserve(recovered, nightly(), LATER)), checkpoint)
        self.assertEqual(state['generation'], 1)
        self.assertTrue(all(v == 2 for v in state['versions'].values()))

    def test_abandonment_consumes_versions_and_preserves_image_generation(self):
        state = helper.reserve(self.state, nightly(), NOW)
        abandoned = helper.abandon(state, self.state['current'])
        resumed_image = helper.reserve(abandoned, nightly(), LATER)
        self.assertEqual(resumed_image['generation'], 1)
        self.assertTrue(all(v == 3 for v in resumed_image['versions'].values()))
        abandoned = helper.abandon(resumed_image, self.state['current'])
        new_image = helper.reserve(abandoned, nightly(b'next image'), LATER)
        self.assertEqual(new_image['generation'], 2)
        self.assertTrue(all(v == 4 for v in new_image['versions'].values()))

    def test_generation_is_shared_by_successful_nightly_and_explicit_release_promotion(self):
        selected = nightly()
        release = dict(kind='release', payload_sha256=helper.identity(selected['image']), notes='Tested on both Pis.',
                       pi_qualification_sha256='d' * 64)
        with self.assertRaises(ValueError):
            helper.reserve(self.state, release, NOW)
        published = finish(helper.reserve(self.state, selected, NOW), self.tooling)
        promoted = helper.reserve(published, release, LATER)
        self.assertEqual(promoted['generation'], 1)
        for role in ('targets', 'release'):
            promoted = helper.prepare(promoted, role, self.tooling)
            if role == 'release':
                image = promoted['pending']['stages'][role]['request']['change']['image']
                self.assertEqual(image, dict(selected['image'], generation=1, channel='release'))
            promoted = helper.retain(promoted, role, role_bytes(role, 3), self.tooling)
        completed = finish(promoted, self.tooling)
        image = completed['images'][release['payload_sha256']]
        self.assertIsNotNone(image['nightly'])
        self.assertIsNotNone(image['release'])

    def test_renewal_keeps_images_and_consumes_only_metadata_versions(self):
        published = finish(helper.reserve(self.state, nightly(), NOW), self.tooling)
        renewed = finish(helper.reserve(published, {'kind': 'renew'}, LATER), self.tooling)
        self.assertEqual(published['images'], renewed['images'])
        self.assertEqual(published['generation'], renewed['generation'])
        self.assertEqual(renewed['current']['version'], 3)

    def test_conflicting_intent_and_changed_identity_do_not_mutate_history(self):
        state = helper.reserve(self.state, nightly(), NOW)
        before = helper.encode(state)
        with self.assertRaises(ValueError):
            helper.reserve(state, nightly(b'different'), NOW)
        self.assertEqual(helper.encode(state), before)
        state = helper.abandon(state, state['current'])
        changed = nightly()
        changed['image']['boot_identity'] = 'f' * 64
        with self.assertRaises(ValueError):
            helper.reserve(state, changed, NOW)

    def test_stage_order_and_exact_response_are_preserved(self):
        state = helper.reserve(self.state, nightly(), NOW)
        for role in ('release', 'nightly', 'snapshot', 'timestamp'):
            with self.assertRaises(ValueError):
                helper.prepare(state, role, self.tooling)
        state = helper.prepare(state, 'targets', self.tooling)
        before = helper.encode(state)
        with self.assertRaises(ValueError):
            helper.retain(state, 'targets', role_bytes('targets', 9), self.tooling)
        self.assertEqual(helper.encode(state), before)
        state = helper.retain(state, 'targets', role_bytes('targets', 2), self.tooling)
        with self.assertRaises(ValueError):
            helper.retain(state, 'targets', role_bytes('targets', 2) + b' ', self.tooling)
        self.assertEqual(helper.retain(state, 'targets', role_bytes('targets', 2), self.tooling), state)

    def test_public_readback_prevents_false_confirmation_or_abandonment(self):
        state = helper.reserve(self.state, nightly(), NOW)
        for role in helper.ROLES:
            state = helper.prepare(state, role, self.tooling)
            state = helper.retain(state, role, role_bytes(role, 2), self.tooling)
        published = record({r: helper.signed(state, r) for r in helper.ROLES})
        with self.assertRaises(ValueError):
            helper.confirm(state, published, state['current'], NOW)
        with self.assertRaises(ValueError):
            helper.abandon(state, published)
        with self.assertRaises(ValueError):
            helper.confirm(state, state['current'], state['current'], NOW)
        self.assertIsNone(helper.confirm(state, published, published, NOW)['pending'])

    def test_missing_corrupt_duplicate_or_exhausted_state_cannot_reset_counters(self):
        for raw in (b'', b'{}', b'{"schema":1,"schema":1}', b'[]'):
            with self.assertRaises(ValueError):
                helper.load(raw)
        for field in ('generation', 'revision'):
            state = copy.deepcopy(self.state)
            state[field] = helper.MAX_VERSION
            with self.assertRaises(ValueError):
                helper.reserve(state, nightly(), NOW)
        state = copy.deepcopy(self.state)
        state['versions']['timestamp'] = helper.MAX_VERSION
        with self.assertRaises(ValueError):
            helper.reserve(state, nightly(), NOW)

    def test_checkpoint_tampering_rejects_before_tool_execution(self):
        state = helper.reserve(self.state, nightly(), NOW)
        state = helper.prepare(state, 'targets', self.tooling)
        mutations = [lambda s: s['pending']['stages']['targets']['request'].update(version=9),
                     lambda s: s['versions'].update(targets=1),
                     lambda s: s['current'].update(version=999),
                     lambda s: s['images'][helper.identity(nightly()['image'])].update(generation=99)]
        for mutate in mutations:
            changed = copy.deepcopy(state)
            mutate(changed)
            with self.assertRaises(ValueError):
                helper.load(helper.encode(changed))

    def test_release_requires_notes_and_qualified_pi_evidence(self):
        release = dict(kind='release', payload_sha256='a' * 64, notes='Reviewed', pi_qualification_sha256='b' * 64)
        for key, value in (('notes', ' '), ('pi_qualification_sha256', None), ('payload_sha256', '../image')):
            changed = dict(release, **{key: value})
            with self.assertRaises(ValueError):
                helper.intent(changed)


if __name__ == '__main__':
    unittest.main()
