//! Tests of the multisig facade against a live dev federation that runs Fedi's stability pool.
//!
//! The federations `devimint` stands up do not run the stability pool, so these run under
//! `devi`, the Fedi monorepo's dev federation, instead:
//!
//! ```sh
//! FEDI_REPO=/path/to/fedibtc/fedi nix develop --accept-flake-config .#wasm \
//!     -c scripts/run-sdk-multisig-tests.sh
//! ```
//!
//! Without a federation every test returns early with a notice, so a plain `cargo test` stays
//! green. `FM_SDK_REQUIRE_DEVI`, which the script sets, turns a missing federation into a failure
//! instead.

use fedimint_sdk::{
    Amount, ErrorCode, Federation, InviteCode, MemberKey, Mnemonic, Multisig, MultisigDepositState,
    MultisigSweepState, MultisigTransferState, Notes, Policy, Proposal, RecoveryState, Sdk,
    Storage,
};

/// The group every member of these tests is in.
const GROUP: &str = "devi-test-group";

/// The smallest deposit or payout the federation takes with its test parameters.
const MINIMUM: u64 = 100_000;

/// The federation's invite code, or leaves the test early with a notice.
macro_rules! devi {
    () => {
        match invite() {
            Some(invite) => invite,
            None => {
                assert!(
                    std::env::var_os("FM_SDK_REQUIRE_DEVI").is_none(),
                    "FM_SDK_REQUIRE_DEVI is set but no dev federation was found"
                );
                eprintln!("skipping: not running under devi");
                return;
            }
        }
    };
}

/// devi's invite code: from the environment `dev-fed --exec` sets, or from the client
/// directory it writes it to.
fn invite() -> Option<InviteCode> {
    let invite = std::env::var("FM_INVITE_CODE")
        .ok()
        .filter(|invite| !invite.trim().is_empty())
        .or_else(|| {
            let dir = std::env::var("FM_CLIENT_DIR").ok()?;
            std::fs::read_to_string(std::path::Path::new(&dir).join("invite-code")).ok()
        })?;
    Some(invite.trim().parse().expect("devi's invite code parses"))
}

/// One member of a group: an SDK instance of its own, on the federation.
struct Member {
    sdk: Sdk,
    federation: Federation,
    multisig: Multisig,
    _storage: tempfile::TempDir,
}

impl Member {
    /// A member with a fresh seed, who joins.
    async fn join(invite: &InviteCode) -> Member {
        let (sdk, storage) = open(None).await;
        let federation = sdk.join(invite).await.expect("the federation joins");
        Member::on(sdk, federation, storage)
    }

    /// A member on another device with `mnemonic`, who recovers the federation from it.
    async fn recover(invite: &InviteCode, mnemonic: Mnemonic) -> Member {
        let (sdk, storage) = open(Some(mnemonic)).await;
        let recovery = sdk.recover(invite).await.expect("the recovery starts");
        assert_eq!(
            recovery
                .progress
                .await_final()
                .await
                .expect("the recovery ends"),
            RecoveryState::Done
        );
        Member::on(sdk, recovery.federation, storage)
    }

    fn on(sdk: Sdk, federation: Federation, storage: tempfile::TempDir) -> Member {
        assert!(
            federation.capabilities().multisig,
            "a federation running the stability pool offers multisig"
        );
        let multisig = federation.multisig().expect("multisig is offered");
        Member {
            sdk,
            federation,
            multisig,
            _storage: storage,
        }
    }

    /// Gives this member `msats` of ecash, spent from devi's own client.
    async fn fund(&self, msats: u64) {
        let mint_client = std::env::var("FM_MINT_CLIENT").expect("devi sets FM_MINT_CLIENT");
        let mut words = mint_client.split_whitespace();
        let program = words.next().expect("FM_MINT_CLIENT names a program");
        let output = std::process::Command::new(program)
            .args(words)
            .args(["spend", &msats.to_string()])
            .output()
            .unwrap_or_else(|err| panic!("could not run `{program}`: {err}"));
        assert!(
            output.status.success(),
            "spending from devi's client failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let spent: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("`spend` prints JSON");
        let notes: Notes = spent["notes"]
            .as_str()
            .expect("`spend` prints the notes")
            .parse()
            .expect("the notes parse");
        let ecash = self.federation.ecash().expect("the federation runs a mint");
        let receive = ecash.receive(&notes).await.expect("the notes are received");
        receive.await_final().await.expect("the receive ends");
    }

    /// Proposes paying `msats` to this member, has `approvers` approve, and has `executor`
    /// execute it, which has to end `Done`. Returns the proposal and its approvals.
    async fn paid(
        &self,
        account: &fedimint_sdk::MultisigAccount,
        msats: u64,
        approvers: &[&Member],
        executor: &Member,
    ) -> (Proposal, Vec<fedimint_sdk::Approval>) {
        let proposal = self
            .multisig
            .propose(GROUP, account, Amount::from_msats(msats))
            .await
            .expect("a proposal");
        // As the application moves it between members.
        let proposal = Proposal::parse(proposal.to_string()).expect("the proposal parses");
        assert_eq!(proposal.amount(), Amount::from_msats(msats));
        assert_eq!(proposal.account_id(), account.id());
        assert_eq!(
            proposal.proposer(),
            self.multisig.member_key(GROUP).await.expect("a key")
        );
        let mut approvals = Vec::new();
        for member in approvers {
            let approval = member
                .multisig
                .approve(GROUP, &proposal)
                .await
                .expect("an approval");
            approvals.push(fedimint_sdk::Approval::parse(approval.to_string()).expect("it parses"));
        }
        let payout = executor
            .multisig
            .execute(&proposal, &approvals)
            .await
            .expect("the payout starts");
        assert_eq!(
            payout.await_final().await.expect("the payout ends"),
            MultisigTransferState::Done
        );
        (proposal, approvals)
    }

    async fn shutdown(self) {
        self.sdk.shutdown().await.expect("the instance shuts down");
    }
}

/// A fresh instance on a directory of its own, with `mnemonic` if one is given.
async fn open(mnemonic: Option<Mnemonic>) -> (Sdk, tempfile::TempDir) {
    let storage = tempfile::tempdir().expect("a temporary directory");
    let path = storage.path().to_str().expect("a utf-8 path");
    let builder = Sdk::builder().storage(Storage::at(path).expect("a valid path"));
    let builder = match mnemonic {
        Some(mnemonic) => builder.mnemonic(mnemonic),
        None => builder,
    };
    (builder.build().await.expect("an instance opens"), storage)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_deposits_proposes_approves_pays_out_and_sweeps() {
    let invite = devi!();
    let alice = Member::join(&invite).await;
    let bob = Member::join(&invite).await;
    let carol = Member::join(&invite).await;

    // Every member has a key of its own for the group, and the same one every time.
    let mut keys: Vec<MemberKey> = Vec::new();
    for member in [&alice, &bob, &carol] {
        let key = member.multisig.member_key(GROUP).await.expect("a key");
        assert_eq!(member.multisig.member_key(GROUP).await.expect("a key"), key);
        assert_ne!(
            member
                .multisig
                .member_key("another group")
                .await
                .expect("a key"),
            key
        );
        keys.push(key);
    }
    let policy = Policy::new(keys, 2);
    let account = alice.multisig.account(&policy).expect("a valid policy");
    for member in [&bob, &carol] {
        let theirs = member.multisig.account(&policy).expect("a valid policy");
        assert_eq!(theirs.id(), account.id());
    }
    assert_eq!(
        bob.multisig.balance(&account).await.expect("a balance"),
        Amount::from_msats(0)
    );

    // Anyone deposits, from their ecash.
    alice.fund(5 * MINIMUM).await;
    let err = alice
        .multisig
        .deposit(&account, Amount::from_msats(MINIMUM - 1))
        .await
        .expect_err("below the minimum");
    assert_eq!(err.code, ErrorCode::InvalidInput);
    let deposit = alice
        .multisig
        .deposit(&account, Amount::from_msats(3 * MINIMUM))
        .await
        .expect("the deposit starts");
    assert_eq!(
        deposit.await_final().await.expect("the deposit ends"),
        MultisigDepositState::Done
    );
    assert_eq!(
        carol.multisig.balance(&account).await.expect("a balance"),
        Amount::from_msats(3 * MINIMUM)
    );

    // A payout below the minimum that does not empty the account is refused up front.
    let err = bob
        .multisig
        .propose(GROUP, &account, Amount::from_msats(MINIMUM - 1))
        .await
        .expect_err("below the minimum");
    assert_eq!(err.code, ErrorCode::InvalidInput);

    // An approval from only one member is short of the threshold.
    let proposal = bob
        .multisig
        .propose(GROUP, &account, Amount::from_msats(2 * MINIMUM))
        .await
        .expect("a proposal");
    let one = carol
        .multisig
        .approve(GROUP, &proposal)
        .await
        .expect("an approval");
    let err = alice
        .multisig
        .execute(&proposal, &[one])
        .await
        .expect_err("one approval of two");
    assert_eq!(err.code, ErrorCode::InvalidInput);

    // Bob proposes paying himself, Bob and Carol approve, and Alice, who did not, executes.
    let (proposal, approvals) = bob
        .paid(&account, 2 * MINIMUM, &[&bob, &carol], &alice)
        .await;
    assert_eq!(
        alice.multisig.balance(&account).await.expect("a balance"),
        Amount::from_msats(MINIMUM)
    );

    // The same proposal does not pay out twice.
    let replay = carol
        .multisig
        .execute(&proposal, &approvals)
        .await
        .expect("the replay is submitted");
    assert!(matches!(
        replay.await_final().await.expect("the replay ends"),
        MultisigTransferState::Failed { .. }
    ));

    // Bob collects what he was paid into his ecash. Two sweeps at once start one between them:
    // the second waits for the first, and then finds nothing left.
    let before = bob.federation.balance().await.expect("a balance");
    let (first, second) = tokio::join!(bob.multisig.sweep(), bob.multisig.sweep());
    let mut started = [first, second]
        .into_iter()
        .filter_map(|sweep| sweep.expect("the sweep is answered"))
        .collect::<Vec<_>>();
    assert_eq!(started.len(), 1, "one sweep moves the payout");
    let sweep = started.pop().expect("one sweep");
    let swept = Amount::from_msats(2 * MINIMUM);
    assert_eq!(
        sweep.await_final().await.expect("the sweep ends"),
        MultisigSweepState::Done { amount: swept }
    );
    assert_eq!(
        sweep.details().await.expect("its details").amount,
        Some(swept),
        "the record keeps what the sweep moved"
    );
    let after = bob.federation.balance().await.expect("a balance");
    assert!(
        after.msats() > before.msats(),
        "the sweep added to the ecash balance: {before:?} to {after:?}"
    );
    assert!(
        bob.multisig
            .sweep()
            .await
            .expect("nothing to sweep")
            .is_none()
    );

    // Carol empties the account: below the minimum, but all it holds. Her payout waits for a
    // sweep, and she loses her device before she sweeps it.
    carol.paid(&account, MINIMUM, &[&carol, &alice], &bob).await;
    assert_eq!(
        bob.multisig.balance(&account).await.expect("a balance"),
        Amount::from_msats(0)
    );
    let carol_key = carol.multisig.member_key(GROUP).await.expect("a key");
    let carol_mnemonic = carol.sdk.export_mnemonic();
    carol.shutdown().await;

    // Recovering her seed on a new device brings back what the federation holds for her: her key
    // for the group, the account, and the payout waiting for her, which she sweeps from there.
    let restored = Member::recover(&invite, carol_mnemonic).await;
    assert_eq!(
        restored.multisig.member_key(GROUP).await.expect("a key"),
        carol_key
    );
    assert_eq!(
        restored.multisig.account(&policy).expect("valid").id(),
        account.id()
    );
    let sweep = restored
        .multisig
        .sweep()
        .await
        .expect("the sweep starts")
        .expect("the payout is waiting");
    assert_eq!(
        sweep.await_final().await.expect("the sweep ends"),
        MultisigSweepState::Done {
            amount: Amount::from_msats(MINIMUM)
        }
    );
    // What the old device did with the account is not part of the new one's history.
    let activity = restored
        .federation
        .activity(None, 50)
        .await
        .expect("a page");
    let multisig_rows = activity
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                fedimint_sdk::OperationKind::MultisigDeposit
                    | fedimint_sdk::OperationKind::MultisigTransfer
                    | fedimint_sdk::OperationKind::MultisigSweep
            )
        })
        .map(|item| item.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        multisig_rows,
        vec![fedimint_sdk::OperationKind::MultisigSweep],
        "only the sweep made on the new device"
    );

    for member in [alice, bob, restored] {
        member.shutdown().await;
    }
}
