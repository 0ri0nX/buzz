//! Root-qualified public source literals; no runtime configuration.
use super::Pin;

pub(super) const SOURCE_PINS: &[Pin] = &[
    Pin {
        path: "/home/orionx/.local/state/paseo-v0.11.1/install/node_modules/@getpaseo/client/package.json",
        sha256: "d2da6e0db765b056fbc81e94afb4ba71adea8d9b79f9d9bf345d09d7ad37fb78",
    },
    Pin {
        path: "/home/orionx/.local/state/paseo-v0.11.1/install/node_modules/@getpaseo/server/package.json",
        sha256: "f4fbd7f6bfdc3e0b511a3fd12df4a359a696121d186007cf36cda877755fbd3c",
    },
    Pin {
        path: "/home/orionx/.local/state/rowvia-management-pilot-v75/scanner.json",
        sha256: "1ecd1fdd6a3513dbc533c6098045e22347d12d829e89779d417a0c6c2353dd76",
    },
    Pin {
        path: "/home/orionx/rowvia-b002c3-production-design-v1.OkZDvWrE/prepare-adapters-v2/source/prepare-authority.py",
        sha256: "795cf79aaf3ac8c9f4504aee30f2c30acc0521fc19a2d5174e8d38ddcf0601c6",
    },
    Pin {
        path: "/home/orionx/rowvia-buzz-native-v0.5.27-20261009/native-root-r2/usr/bin/buzz",
        sha256: "2e6a266b9d3b45f3d62b873fb0ee2679d9066d507ecc05a9d5dbb3b472ba12f5",
    },
    Pin {
        path: "/home/orionx/rowvia-buzz-native-v0.5.27-20261009/native-root-r2/usr/bin/buzz-acp",
        sha256: "c73b9f43c5645f437671a06a1f804aad04c6436558a7fc9e7636e4091a035b3c",
    },
    Pin {
        path: "/home/orionx/rowvia-core83-socket-build.HabTa8X6/artifacts/core83-socket",
        sha256: "800f6749d19c69122a54c7b15f8a1f9edd127a5ad18c5dddab3a93eb2bb2d6fb",
    },
    Pin {
        path: "/home/orionx/rowvia-registration-cli-3ce4752.pqbQBkOz/artifacts/rowvia-context",
        sha256: "ca1ccb7ff94f3d17ddf2daa0c7b1c1daee0d20927d712830f9c14eece090cfae",
    },
    Pin {
        path: "/nix/store/h9aczmrkpcmm9z47648m5n9sg67vi5gw-postgresql-18.6/bin/psql",
        sha256: "81b0b6708586f69949759567f90296643f6b6e82f8e407919f2c3adb55d6462f",
    },
    Pin {
        path: "/usr/bin/dash",
        sha256: "781c19b56fbdb17284707f9026e107f639e5447df7df3b248a5d5a50c4b0806c",
    },
    Pin {
        path: "/usr/bin/flock",
        sha256: "008fbee31a58132fa80a585bca7727f292613fe410a4eea1e095589d3555e454",
    },
    Pin {
        path: "/usr/bin/python3.13",
        sha256: "21769fdaebef754faca4138fee289f86872cc601d6f8c6f29ea3c9cc92827d2d",
    },
    Pin {
        path: "/usr/local/lib/node_modules/@openai/codex/node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/bin/codex",
        sha256: "399ff69c431c066e40de318fba45c2b07f45a76b19939d247ee4828fc70c4215",
    },
    Pin {
        path: "/var/tmp/rowvia-bridge-mvp-metadata.IYcSqs/rowvia-buzz-bridge",
        sha256: "72aa85878382f918d8cc94f4c29886d533c1b4604d6cb5ba58a8c706f12aac76",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/compat/normalize-provider-models.js",
        sha256: "088295589fcdc5db22c897a9ef568b5ff0aa112df045bebef53763abc94823cf",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/connection/index.js",
        sha256: "f2140c5b6c11d736e0000e7ec62e8a6f3c10c9556505b48a47d991055dc607a9",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/connection/legacy-workspaces.js",
        sha256: "13f6f871251b3d4c4c28ba02c3333b0d5ab4419b1cbe6f75b1cea7e7634d1277",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/connection/legacy.js",
        sha256: "45f89d8b47957bee5d9306077e50bf76e07dbf1a13c732962f7af6daa6eb2899",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/connection/owned.js",
        sha256: "ff54c49919d431e994bec21b86784ac9116083a792140d89c28177179675bc91",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/creation/index.js",
        sha256: "0430cdf440e1922f82998b1c351ef5f13c1b5b7903b3edb847b736bb3d46eac1",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/daemon-client-relay-e2ee-transport.js",
        sha256: "679b7fa6ccfaa23f5a29cf7febb3710ecd54ec1d275d6a0be553b29a079690ec",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/daemon-client-runtime-metrics.js",
        sha256: "48a72b4d6847fefbc27a64e350a834f0d87743303b2bd4f191071df0ef124f7a",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/daemon-client-transport-utils.js",
        sha256: "f62cfd222cc2278b8290d4c76f8b99439a6e198868e35a7fd8905d6967fea4c0",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/daemon-client-transport.js",
        sha256: "54020e3c7cf43c23705da692b9179b1179f374073d6140a19a320fb410056695",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/daemon-client-websocket-transport.js",
        sha256: "deb4d4e45b48c2061e23a1acad88bed2f65035f41d9c5d7b993dc4e37d3b8c7f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/daemon-client.js",
        sha256: "d6a512d871b8ba3f8f8334404bd4eedf75ccb6de4cef5bd84d96fcb9b3b144fd",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/index.js",
        sha256: "16b873d2ae45195597765f351ca1af62560a77d4bf42300008b26d7be22afd38",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/legacy-usage-icons.js",
        sha256: "6674c20ea6de3d562101957a450731aab5a702a75c71869e8fbc40dd2e25be8c",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/provider-snapshots/index.js",
        sha256: "8f20739ab57646f9c7cc5c4955ef7a8fcbe3596c80fd0bff582d8b0d40c6490b",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/terminal-stream-router.js",
        sha256: "69e660cb2ca2afbf28b9490a2e660239df7ee0394985b436f50820dd1775d4e7",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/terminals/index.js",
        sha256: "2a328b6b07d970b3136493869acaf7c87e946aba32a8b86ad588abcdb86bf9b4",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/dist/timeline-subscription/index.js",
        sha256: "cdbdc1f10b19470c1396cd78c5511d7135cb0cf5956cf4c40650df2125546eb7",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/client/package.json",
        sha256: "d2da6e0db765b056fbc81e94afb4ba71adea8d9b79f9d9bf345d09d7ad37fb78",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/agent-lifecycle.js",
        sha256: "fb275fd9b9144e83c96e359f3e580c62a2a880b8b6411ebf01142af5b2252e54",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/agent-profile.js",
        sha256: "07fb9cb22a0662d4c0c1c646354f4640cd85448312820e80471ce9c257db17de",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/agent-state-bucket.js",
        sha256: "aff323aa517122b4361e5462b7ac971c5ae79d30190d0bee8f17c9c072364b25",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/agent-title-limits.js",
        sha256: "0dbfbc541eb452fa117a1cf70c66d2011b35f92cc232763fea5ee6b5d7e0fed8",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/agent-types.js",
        sha256: "f7048e275f63c83b2512b46807fbf6e4e722dbd83cb580c1314c19e6931d74d6",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/binary-frames/demux.js",
        sha256: "0e960d177cc2133916247d713cb6064a751391c76bb626b523f1726916ea1aad",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/binary-frames/file-transfer.js",
        sha256: "4bf534d8a0b7de9b74ddf29ee8f105cabef2e68ea47fb94466412d9214ef5387",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/binary-frames/index.js",
        sha256: "3af06230bf356743235f87317f388d9205a83dfb736be02ddb53b408347c1f58",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/binary-frames/terminal.js",
        sha256: "b06cc568118e46345e29bf426f5223cef846af2435eadbfee99e1ba660baebb7",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/browser-automation/capabilities.js",
        sha256: "81b57c02f470abea80d9ed0bf3196b16531c712fc7e1640289a88e049ced36e1",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/browser-automation/rpc-schemas.js",
        sha256: "e8f3a44d5acf9b3379f58fa637ae1c73b0f5e25a0feb5ec28d291fd9c4dd37bc",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/chat/rpc-schemas.js",
        sha256: "219e276a9077e8e98a37bf4053baefbbd7ddb7a0c14f0d68d0d8368ba206b35d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/chat/types.js",
        sha256: "8bc31c69a94eb28579139ea5f85b422fb616480a7962c63542e7e6f73824de6c",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/client-capabilities.js",
        sha256: "535503756aca47014064e17dc2a7ccdd1f05e251876950658cdd1c357f09ba0f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/daemon-endpoints.js",
        sha256: "c6244609b37ec2008cd8a8198ed82a7f3a27a86bc4c6d84c6f81e8ec6a237f5f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/generated/validation/ws-outbound.aot.js",
        sha256: "8b3b87dcac0a9d7e5da78c93f0cd1d5aeec2d6f57253e0bed8a015eeb05b8a05",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/loop/rpc-schemas.js",
        sha256: "099cb3fb372f90049648b69deb99990762d6f0d6083f0d09fbae00cb5fc03a07",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/messages.js",
        sha256: "6052ab239118618f71235728af0c1d66524a68ade36133bab7f6e9480ca3b2cf",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/paseo-config-schema.js",
        sha256: "46196b354916602f8cadb87fe4690755c2e98d5305648864841cab41309469c7",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/plugin-config.js",
        sha256: "0f60f870d62d9a07d22362d5354fed27dd6c92822f57dccb07671ded14117ad5",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/plugin-registry.js",
        sha256: "41f9ddbb4587f5d3c2207ca32fc6bb639074a09f990bf96a84e180c31f66834f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/provider-config.js",
        sha256: "b947297aa8c46f676fb61fde02f6792c06169e3afdd7cf96bd47f75ce7627305",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/provider-manifest.js",
        sha256: "0af826339f7ab4a587ac893e47160f5a273d850e74586f8999c66c0561ee3921",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/provider-snapshot-codec.js",
        sha256: "fcc05322c3dc972a155cab80ead4d82faaf6e7acf7ecea6f2112b23172f62a52",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/schedule/rpc-schemas.js",
        sha256: "820ba6c34c793669a4859777c7862f4f8721836feffd0497e3f881e30744e13f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/schedule/types.js",
        sha256: "923ea27323008d7ae37731b3c6fd019bb510ff123216eb19dfe65186bd2a62bc",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/terminal-activity.js",
        sha256: "27dedd07115476c8c68601bd3036b87f5591bb353e04c2baba7de0b56554a539",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/terminal-profile.js",
        sha256: "6d9844f791ad63f39df9799eaf81e08059dcd070c0f3477dc5c94232cd4056e2",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/validation/ws-outbound-schema-metadata.js",
        sha256: "69f5dfbe58f70dddfb9beadab69fd4106555028db7d393f940a69f4ba2febef2",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/validation/ws-outbound.js",
        sha256: "24f56fbe45afae0922cbccf8be61e729c746ef2c4af9fffc6ed1c538e3842eba",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/protocol/dist/workspace-labels.js",
        sha256: "dfaf7c4ad3c1ac2755e861b9dcb5a8f42c0cdde581c1ebce880da989b9b8e02d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/relay/dist/base64.js",
        sha256: "26c8947057483b6939ea58d712617ac5f9ff95e720404c8a0e779e105152bb38",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/relay/dist/crypto.js",
        sha256: "7d45536ad443b045c24b8b5d897bc937486b6e4dc7e17752312fa4dc6a919882",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/relay/dist/e2ee.js",
        sha256: "3d2dab0c88aa6080d1f3850fc6e3073fcd4ef2dafac78a166ec541c3c9ebd5da",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/@getpaseo/relay/dist/encrypted-channel.js",
        sha256: "55f5dcf2ba980a3999dd1baaec9f13a611adeeadea4c77d10b068360290fefee",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/base64-js/index.js",
        sha256: "829eadd8a1a441d25be0cb93b00e16a0d0c20fd294db95d8f2ed87e6954b7182",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/tweetnacl/nacl-fast.js",
        sha256: "6bcd37a3b20dce913f82d4b23e4e2b661058b4b953df8a3f8c45d56ac4f72447",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/buffer-util.js",
        sha256: "8b0a45739132f82e25ea13163780abf547ccfe989267f3eb7abb475beec92da3",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/constants.js",
        sha256: "391e823142b8b370e55a2fd32b022deaf03b8415c56000009674ebc86a0b4f86",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/event-target.js",
        sha256: "c45d3c6e12d170c860c0c3f1a050aa0f864d9806632b609a1e607d675aba128c",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/extension.js",
        sha256: "852564f0f6b460287043803eae732666fb5610f676874354fc89f06aa4e986ed",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/limiter.js",
        sha256: "e0469d4b83f6ba764b15f80e1766b75c136fbff68f048f4c050f0b1c7f065f69",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/permessage-deflate.js",
        sha256: "16a91536988a53c23047ee5882392068728a839a244178c3b09b8c54982f58b0",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/receiver.js",
        sha256: "a6ada33c9a2d54eaf595824b821ea89ca2c6395f89aa06755c52142148274884",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/sender.js",
        sha256: "d0791d30c3defd44dcabbddb879a901c757993fea2c00a7ffea01d53b23b4b77",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/stream.js",
        sha256: "a56fcb6e2b152097ee820b6f5410b1f71e59819b45d08d9f8bda588fe39070ec",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/subprotocol.js",
        sha256: "be3f6323d6f549568577dcba9004c1479d95c65a7abb0fe0c582875b9fac0b7c",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/validation.js",
        sha256: "41ce8e83d0d434132e1704895fedb91f6703a701b42d91c80954ab29b2845593",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/websocket-server.js",
        sha256: "6d8dd89a841748c80445fd5fcdb5ba8005369b53cc3525135e11304c8b9c55e5",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/lib/websocket.js",
        sha256: "7ccf305925f0cdb74e06b75b4c84de134b1bd771a220ef409b24520def08128f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/ws/wrapper.mjs",
        sha256: "fe154662301fd558f935f9c217fccbe7a7dac02ff39e648a000589403ff27c7f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/index.js",
        sha256: "32e7efd49a94a765f6d10a1a0a3c02c50c246def17c80c6f0225f85525d98505",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/checks.js",
        sha256: "7543391a9ad5e7df287bbfecaa79844d46af44c75c1de0d43a411d65795d9dd1",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/coerce.js",
        sha256: "fb2efe3b6eacc475c77cbb571b5fb8650d92a10f316dc869e49b722973d83c3c",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/compat.js",
        sha256: "0e948b101771cc0e7b753e54b3f5b486929d3db9bd4c09d98fe4ce86271bcf7b",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/deep-partial.js",
        sha256: "0abffcc9838e04dc6f091229c1d522fbfd43aab9563316b52d3eb8ff2bacf26d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/errors.js",
        sha256: "d32f7c1ef8e2c70072f6f0ec8ba7081bbc331353cc3a7cb4429e5ece91409543",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/external.js",
        sha256: "c6b584ae70ba4e7132b2cd78c23ea4ecedc9828abf999edee312372e1fb453d6",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/from-json-schema.js",
        sha256: "31dc999a26cb6928f5e99ea564d648567e85cf9aa47565198fb6186dbd6c070e",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/in-out.js",
        sha256: "34370913842ff02d14a9796b11c5ec2af160bdb35a7148edc286e9a9c6fb5c17",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/iso.js",
        sha256: "deaa026b7c5302e59719f401248e7866691d8d8a892884dd2b6868b1cb7252d8",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/parse.js",
        sha256: "11941573401bbef138842c7a523dff554873343fccbf0e8b0a144259d2d703f1",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/classic/schemas.js",
        sha256: "2bf4a631a18f734a9a7ab7fc11bcf919214774d2235a312dcec0dd53d0324c32",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/api.js",
        sha256: "03ae6016abc9585e3f3481d6b8d775a95ae4ee65badbde73ca1ed953ff14c0e7",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/checks.js",
        sha256: "bc42543274218aa7ef72192b3218d2a4a399e53df8de240a04d9ff275096f563",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/compile.js",
        sha256: "df28853593428e5a2a8729b09f43baf0998eaae7f1e37797993035187833b054",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/core.js",
        sha256: "c7ce485c97263e42568d16b11bb4a90133221c7e807498a9273d70f41ba16ef6",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/doc.js",
        sha256: "be4c1b1a1ad247cc70f360a572060edd7e0cea24252c3a319f33d247899c5643",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/errors.js",
        sha256: "a0c95ef2f7085b5bc2931f7b566f524b26a0360c8df741e3f87f923745cf9792",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/index.js",
        sha256: "2a0e452323aa5b75b7d7de4879289e2579b9dd2d6af291c0416ba6806ca2199d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/json-schema-generator.js",
        sha256: "beaeab6faa654e99e443b8a9f5d8a4adc08cd8226f92a0cd32477a58ecfa1807",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/json-schema-processors.js",
        sha256: "b6d2aee72212c076b9cde38da6333591c76a24eb049a4ee4e7bd2b7825d8b817",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/json-schema.js",
        sha256: "8e609bb71c20b858c77f0e9f90bb1319db8477b13f9f965f1a1e18524bf50881",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/memoizer.js",
        sha256: "18a3188c51229d9189a5408b0b7fbc3e121b55c2b4c3c406d79cb9b77e7ad964",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/parse.js",
        sha256: "fb05b91ec260f3f484f7c00a177b6126ceb8fd1b09dc90b1200afdffd4d98efd",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/regexes.js",
        sha256: "24fc98f6fc9175e50ff29d28b040e3b4ac5650de19914d45ee27b853d367ecc6",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/registries.js",
        sha256: "f04d4b2c0bc5b4ac2d6610ea34edf2e5260eade18f20976b79916dcdbeb9fa4d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/schemas.js",
        sha256: "e632b655441418f0593153abc6809fd81cf3e0782d810622d0a4fa50ef6e182e",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/to-json-schema.js",
        sha256: "85b883eb8f05e9f060c71a39fce7df9345b566af29d5bc9c8fa477fe616a051d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/util.js",
        sha256: "728f52b496620336775eafcdb91c0a574380086e53f215b55c58eedc1422c49c",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/versions.js",
        sha256: "63b38035ff5e2a5a5cfe5665e271af72b0cf9791e770dccf5e99d977064fb96c",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/core/visit.js",
        sha256: "a804062900869e09ea0b975cfa569fd3c70dc33bad7fc30b63872d451cae29f7",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ar.js",
        sha256: "eec4016c2b3063ae929617d90ec609e4c425e60c97a954d3a24de0f457b6ec97",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/az.js",
        sha256: "5bd03646b9d18b73e71356cd28937e4e6413b806bb23e14689cc88f6f65ff3a6",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/be.js",
        sha256: "f969afea5d9800a71958a621658943b85a2576a2bfefdd5dea70afc4924d4af4",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/bg.js",
        sha256: "96741333a790320e05aef7286b5b709a4a7c48cfffc50fc5e6acff5b505275e9",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/bn.js",
        sha256: "6d8227b52a6864465b7c52a9ef7e09d261d904ec8a619932e15dd9f9f826e8dd",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ca.js",
        sha256: "67f6f38b59a6e3ee81cc5155480465331dd9a142b2471a495503a81dafba0684",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ckb.js",
        sha256: "bdb9ef0cfda4009f8892585a1780fff863cd409931d6f89394c271042c131137",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/cs.js",
        sha256: "4ffa0e484ad1842a2c0e5c7f2dfe9160155ff5be4b95ab6e524d3a30a2210488",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/da.js",
        sha256: "29addcd60e88422bed9ed78c6c864220a15c06b31fdb4ab7ad4f87be4098e6ae",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/de.js",
        sha256: "f344eb96ba18d9db907f3f9c95897ce2590ab0cde1553c023daffbb1cb5ef6f4",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/el.js",
        sha256: "fdea833aa0a8c381d26216db1521dd7fbc4dccb6d3aa50f9a585ffb975df91eb",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/en.js",
        sha256: "d5930e10d137b048b37823e168e4177947e1f696ada8419c71bb0d32da4333c0",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/eo.js",
        sha256: "97797bd22319ecc3f2044da6c305f1114b851803697029ffce2b66644757da3a",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/es.js",
        sha256: "12e762b921b1e3fdb8e265d2b554de81ce9706ceb23a81527f48ae2dddf355db",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/fa.js",
        sha256: "60e7eeff4c000c85f8420168374344305d3c2110a5f91a2fb672b2a2381eeddf",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/fi.js",
        sha256: "1e72af4193e564cd37dce4a850dc18bc1bcd6ca280f29e9f5f8c6bbab6a2cad9",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/fr-CA.js",
        sha256: "a2ca0451ff943766bb6a88b392b25447462a1d3ed75c1adda5ddb0bc474bcd42",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/fr.js",
        sha256: "477345d6401d0e4f07794b1a2cff8b78953723f1fc8cfaeb6b660c19be47125a",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/gu.js",
        sha256: "b1921ac6b4158e6360a8da6099df8bbf2d7bce7c1cf381d5a5e47aa0c74874d2",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/he.js",
        sha256: "1cfff31fac19616d900d1f3febd9690ea87711e13b5be299326ebfd3f33aad4d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/hi.js",
        sha256: "c31e466c2a31dc10ba6a288e1aebf429a6e9566a26918d0240aae058693f6329",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/hr.js",
        sha256: "ff2bebef0761f2d6095103d58484e3b95551bc51d56770db8f161b8e0e0ec941",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/hu.js",
        sha256: "9816c21ee0b3be9d973f9e033e9587a608f2d35dd1b984d415b88e03242b0189",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/hy.js",
        sha256: "daf6d0c590d9b061759598ba9f7f97041e2874c7a58da2c64b86f152840d68fa",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/id.js",
        sha256: "e5c23bb0b6f696fa936bafd7969e64738504cf7a5be3ec04b24ae899d4912b58",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/index.js",
        sha256: "7ea85250d1e7bd9459cda61f38c78757a410d561a5421bb3f08019bba79c9849",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/is.js",
        sha256: "1cce1285bbf55c8a76303fea0be0fb6d39fbd0ece833137ea23a22f406ac1ae1",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/it.js",
        sha256: "096b497df6c5fe7de4422482b7733e764e4b344e8d7f768a7289556687acdb82",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ja.js",
        sha256: "2a6bbbd6dbd573687b22f35873a13cdf730c07423b4ab114e5a8b12d8777146d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ka.js",
        sha256: "baf3204123cf4246df654fd1948b6c8da6917e7a06e470aeeaf7c1eaa7003628",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/kh.js",
        sha256: "78c3d1991041bb8c8fc0f7c5d745aae422d4c2e4e8f30bae44f3fef4765900a8",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/km.js",
        sha256: "e61079f14070dfa9c7b2047eb79f5bea6d4e845f192756d103c121bbd63ee18a",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/kn.js",
        sha256: "884b1b68a1ca02fe9a774b8de9cbfa3486892eea36192699120f57b31f0a1f42",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ko.js",
        sha256: "11fcef61a2f23e010bf0f569d71be0d49524afc97f3072cc03fdc6fe3ea2f14d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/lt.js",
        sha256: "d2e67a35a3c6e7241a9eb9c5851e4b5b651ac8b23278854862e6055829946c48",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/mk.js",
        sha256: "64ddd914c432ba5690d0686bff6603651ce886a891f84823d7dc766c9c980cd1",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ms.js",
        sha256: "94faf0d306ffa2a4f95e57f3e53060d27220a639b9cb529f00d1d01b6270f045",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ne.js",
        sha256: "6da69989b702ae15baa27b799b6b2b32fd011ba3980a190dd0eccfc9233083d6",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/nl.js",
        sha256: "296ced8be508c5f9e82b562049d2974882029bcd5b2ab4ade14401b33ec99909",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/nn.js",
        sha256: "bae9756817629db30841eb54a27ed9b3c71c2a4208314d8d070be624bd2e9359",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/no.js",
        sha256: "f358e5586940350ee25a05db184565d23eb1c66903e182ebec812172f31cb55a",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ota.js",
        sha256: "66ed8c3ce00b2e9971ed5a17f002937405d8cdfeb399ebe012db9d2317f42146",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/pl.js",
        sha256: "b7f82cf9f747785beeec05cd31d3d7493c52bd3647598924141568f148c07222",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ps.js",
        sha256: "1c15b81fcce0cdc59dbcca18df5c4ecf4e176fc3fb7668207b9ba54df43cce46",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/pt-BR.js",
        sha256: "abc52db767b9dd938954776728ea7480cdcab569a5a17664f60e5a2c1d63e60b",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/pt.js",
        sha256: "93a820f60d571607bf34b69198f5b24617475584c0b22c0f8d160fcdef096a46",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ro.js",
        sha256: "54751d2866171c9c7ce1a72868dec09efa54d1b2b67401767594c5b360aca38a",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ru.js",
        sha256: "40a019b53bd1684fd4b750db87021520dd5d710f714010c0a594e5bcf5787478",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/sk.js",
        sha256: "ae7c49242e70149434c9051dd00209afbd517d98ba54c13692371a5340b616da",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/sl.js",
        sha256: "490ca48f8aae3aed660758b32856d49a9fc5c7e255dad220e75abaef8c35140b",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/sv.js",
        sha256: "448f36996bb0d159489b34ad7d86df03e520a13e43b1c98ea2fb18d7ed4a3c95",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ta.js",
        sha256: "8b6e1bc033fdaefb1acfc7edf88c9f53c3a50cc892066effdab306103e994d42",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/tg.js",
        sha256: "9a5c9a3b18b8ccded28d66033575b22d823df17a7afa8bb9a733f31fe071462f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/th.js",
        sha256: "913cc4962ddf5cb54ed694d04b7008493ef401d3333b26426b32f4890712574e",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/tk.js",
        sha256: "445abfed18c5a9d79143b02c9cf9c171b41fb5dff8ffc2cb59b7aef7203e5a7f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/tr.js",
        sha256: "bca07607a27f518f19877dd9c2235fada9c73eb56896f0f6e274781fab71d64e",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ua.js",
        sha256: "9391169ffbc72f777a83230b6ffcc3f4cf46ab5945ec540de9ef4609dca1f4cc",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/uk.js",
        sha256: "1e59ba41296aeb5b6142a6e5ed84712931e33e54ab0b9ae5d83faf436fc3d968",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/ur.js",
        sha256: "d37ed2c93408e0a6b6e1cd290117a6f0e9d7b2c83509cd34b502e1dd94899c87",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/uz.js",
        sha256: "2cbe07635b03064ffad0bfcc98d2d570f3264b9cd3e22bbd5c160cd0343f7872",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/vi.js",
        sha256: "15939adaf263e0869c602d4b34c206178a235b8bec11c8207ccd3fa50de3a141",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/yo.js",
        sha256: "88b33f6527c1a2faaf159568330cf727c448dc468eac72142a6fb24f8714ac1e",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/zh-CN.js",
        sha256: "8fc02f2bd4d92f46b24b8d82da0ee95c962fc80c14dd7dd469500e14de08d320",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/sdk-public-snapshots/node_modules/zod/v4/locales/zh-TW.js",
        sha256: "01d9b119e318d8b9f1a8fa720d06424457429425a61d0730984b750e6a5edf7d",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/fixed-installation.mjs",
        sha256: "cc7669c086c226661cbe554ad1608658e4ea997436ad6872d23e8a67895c3612",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/fixed_root_data.py",
        sha256: "2ecec7d310227cde99316d8f81d7334948cdc36280167187637e76afedb09c1e",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/management-pilot/memory-mcp-launcher.py",
        sha256: "22f2e2c38ebd4fddafa471271a5273950800e77a5d51698d94cfd60ed5d5e848",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/management-pilot/saved-agent-binding.mjs",
        sha256: "cd61af3398bf535d53ec5d7e61f5d986ed8c9995b1032bcaa18b5654daad156f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/management-pilot/saved-agent-local-worker",
        sha256: "55bdc8072c61b78c3f127b88a88e842b22a776516ef85b7c9f79599557284319",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/management-pilot/saved-agent-runtime-profile.mjs",
        sha256: "3e692723f82c126a34a80e6768bf12cbc16cddaabd00cad38fe87aec1cd56ca2",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/management-pilot/saved-local-registration.mjs",
        sha256: "ec6fe17dd38cbd853be0ba6bc84d681aef885feeb6180b90a75c001c6cbb3738",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/paseo-pilot/architect-native-evidence-adapter.mjs",
        sha256: "69ff1f273e2a7fb3da3e9d71fecea0eccf6138dcb2414720dc362b5b020f0005",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/paseo-pilot/architect-sdk-adapter.mjs",
        sha256: "0bb1fa632651f768b6b07cb30d16593d8112727d78a82739190984f79cd10ee9",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/paseo-pilot/claude-saved-mode.mjs",
        sha256: "5129b0c3c9f6546e4bf72646ce00ffa94d0fddd75b5d6c99519acc2a31f47f86",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/paseo-pilot/saved-agent-acp.mjs",
        sha256: "fc0ea0be3eecd1e5a5413fba252af563abf7cf80da64ce1ecbd1fd0f7baa5d4f",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/paseo-pilot/saved-agent-permission-custody.mjs",
        sha256: "6472c6d47c853d33a7697b1a50e4e005b5fd6344184d284a1ae16a17a0f39649",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/main/integration/paseo-pilot/saved-agent-transport.mjs",
        sha256: "4291cf9684cf418af7864ccfee153249ae3b0bfa835cb608ec2e505aaa59e51b",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/native-callback-client.mjs",
        sha256: "9c3bf6e7a5aab8cc791b8513789451a826691122dc144d087750e7d0a028ca2a",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/owner-entrypoint.mjs",
        sha256: "ff2383b0ddb203121524da3cad00614f0bdfc2ec6af2540172868ce4b1ccc866",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/owner-host-adapter.mjs",
        sha256: "21dec0727d772187cfe50175fc043a89020040718ae26f0aebe03fc36f0b5f4e",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/owner-python-child.mjs",
        sha256: "9f8c04d1c9a02eb7f98f702bda0d8e16f234241b7c1c0dab1d97bb65448872ca",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/owner_authority.py",
        sha256: "87b778462060e61df39c71ea36d925656bd6bd29b51ebe7a89897e3bf56c5a52",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/pinned-closure.mjs",
        sha256: "dd5d2296e47f75cfff106b240e81ef8064f7fb65f534f97fc9f23547c0174c92",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/pinned-modules.mjs",
        sha256: "2dea28896c2c6bf27714fa679a7db5ef45c15c993d7bfc3737b08e994d41febd",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/pipe-transport.mjs",
        sha256: "87dccdab88e22d8cb68e232e2260f1b8adcaaedff6e9782e9f5014c8949f3eea",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/prepare-entrypoint.mjs",
        sha256: "4b0154f7e18e2b17e22fa9c945a5fac684a4e8e6c78fdc643ec8d0a57b48a276",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/qualified_host_authority.py",
        sha256: "8aa437809fb9f662c8a29408672ba943457bc8c8ad1ae313428ec9b877836ad2",
    },
    Pin {
        path: "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/saved-local-frame-parser.mjs",
        sha256: "f08f5b9731845acfd74f626f56f5597d8fd2803300fdada58a6210a968f750ff",
    },
];
pub(super) const SHARED_GROUP_CLOSURE_SHA256: &str =
    "2ca72d02a668fe2a84143679a1dd659cda67b473ce8bf48a4f9152791ecf8fa9";
