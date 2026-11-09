// SPDX-License-Identifier: GPL-3.0-or-later

//! The phone API against a real desktop daemon (with an in-memory clipboard): what the roadmap
//! asks of the Android app (M3, M4, M6), minus its UI and the phone's own clipboard.

// Test code: a panic is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pasteget_core::clip::mime;
use pasteget_core::identity::Identity;
use pasteget_core::pairing::PairingInvite;
use pasteget_desktop::clipboard::{Content, MemoryClipboard, Raw, RgbaImage};
use pasteget_desktop::config::Profile;
use pasteget_desktop::daemon::{Daemon, DaemonOptions};
use pasteget_ffi::{ClipDelivery, PairedComputer, PastegetError, PhoneClient, PhoneListener};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

/// A real 1×1 PNG.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0x60, 0xf8, 0x5f,
    0x0f, 0x00, 0x02, 0x87, 0x01, 0x80, 0xeb, 0x47, 0xba, 0x92, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    use image::ImageEncoder;
    let pixels = vec![128; (width * height * 3) as usize];
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .write_image(&pixels, width, height, image::ExtendedColorType::Rgb8)
        .unwrap();
    jpeg
}

#[derive(Default)]
struct Recorder {
    connections: Mutex<Vec<bool>>,
    clips: Mutex<Vec<(String, Vec<u8>)>>,
    /// The `sensitive` flag of each clip in `clips`.
    sensitive: Mutex<Vec<bool>>,
    deliveries: Mutex<Vec<ClipDelivery>>,
    /// Names of the computers that unpaired the phone.
    unpaired: Mutex<Vec<String>>,
}

impl PhoneListener for Recorder {
    fn on_connection_changed(&self, _device_id: String, connected: bool) {
        self.connections.lock().unwrap().push(connected);
    }

    fn on_clip_received(&self, mime_type: String, data: Vec<u8>, sensitive: bool) {
        self.clips.lock().unwrap().push((mime_type, data));
        self.sensitive.lock().unwrap().push(sensitive);
    }

    fn on_clip_delivered(&self, delivery: ClipDelivery) {
        self.deliveries.lock().unwrap().push(delivery);
    }

    fn on_unpaired(&self, computer: PairedComputer) {
        self.unpaired.lock().unwrap().push(computer.name);
    }
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Pc {
    runtime: tokio::runtime::Runtime,
    daemon: Daemon,
    clipboard: MemoryClipboard,
    _home: tempfile::TempDir,
}

fn start_pc() -> Pc {
    start_pc_in(tempfile::tempdir().unwrap())
}

fn start_pc_in(home: tempfile::TempDir) -> Pc {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let profile = Profile::open_in(home.path(), "pc").unwrap();
    let clipboard = MemoryClipboard::default();
    let options = DaemonOptions {
        listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        advertised_ips: Some(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]),
        poll_interval: Duration::from_millis(10),
        announce: false,
        notifier: None,
        hotkeys: false,
    };
    let daemon = runtime
        .block_on(Daemon::start(profile, clipboard.clone(), options))
        .unwrap();
    Pc {
        runtime,
        daemon,
        clipboard,
        _home: home,
    }
}

/// Stops the daemon and starts it again from the same profile, on a new port.
fn restart(pc: Pc) -> Pc {
    let Pc {
        runtime,
        daemon,
        _home: home,
        ..
    } = pc;
    drop(daemon);
    drop(runtime);
    start_pc_in(home)
}

fn path_of(dir: &tempfile::TempDir) -> String {
    dir.path().to_str().unwrap().to_owned()
}

/// A phone paired with `pc` and connected to it.
struct Phone {
    client: Arc<PhoneClient>,
    listener: Arc<Recorder>,
    _dir: tempfile::TempDir,
}

fn connected_phone(pc: &Pc, name: &str) -> Phone {
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let dir = tempfile::tempdir().unwrap();
    let client = PhoneClient::new(path_of(&dir), name.into()).unwrap();
    client.pair(qr_code).unwrap();
    let listener = Arc::new(Recorder::default());
    client.start(listener.clone());
    wait_until("the connection", || client.connected_count() == 1);
    Phone {
        client,
        listener,
        _dir: dir,
    }
}

impl Phone {
    /// Waits for the `n`th clip (from 1) and returns it.
    fn clip(&self, n: usize) -> (String, Vec<u8>) {
        wait_until("a clip on the phone", || {
            self.listener.clips.lock().unwrap().len() >= n
        });
        self.listener.clips.lock().unwrap()[n - 1].clone()
    }
}

#[test]
fn phone_pairs_then_shares_text_and_screenshots_with_the_pc() {
    let pc = start_pc();
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO M8 5G".into()).unwrap();

    // Scan the QR code.
    let computer = phone.pair(qr_code).unwrap();
    assert_eq!(computer.device_id, pc.daemon.device_id().to_string());

    let listener = Arc::new(Recorder::default());
    phone.start(listener.clone());
    wait_until("the connection", || phone.connected_count() == 1);
    assert_eq!(*listener.connections.lock().unwrap(), [true]);

    // Share text: it lands on the PC clipboard.
    assert_eq!(phone.send_text("Bonjour du téléphone 👋".into()), 1);
    wait_until("the text on the PC clipboard", || {
        pc.clipboard.get() == Some(Raw::Text("Bonjour du téléphone 👋".into()))
    });

    // Share a screenshot: the PC clipboard holds the image.
    assert_eq!(phone.send_image(PNG_1X1.to_vec()).unwrap(), 1);
    wait_until("the image on the PC clipboard", || {
        matches!(
            pc.clipboard.get(),
            Some(Raw::Image(RgbaImage {
                width: 1,
                height: 1,
                ..
            }))
        )
    });

    // A JPEG screenshot is sent as it is, and decoded by the PC.
    assert_eq!(phone.send_image(jpeg(3, 2)).unwrap(), 1);
    wait_until("the JPEG on the PC clipboard", || {
        matches!(
            pc.clipboard.get(),
            Some(Raw::Image(RgbaImage {
                width: 3,
                height: 2,
                ..
            }))
        )
    });

    // Something sent from the PC reaches the phone.
    pc.daemon.send(Content {
        mime_type: mime::TEXT.into(),
        data: b"from the PC".to_vec(),
        sensitive: false,
    });
    wait_until("the clip on the phone", || {
        !listener.clips.lock().unwrap().is_empty()
    });
    assert_eq!(
        listener.clips.lock().unwrap()[0],
        (mime::TEXT.to_owned(), b"from the PC".to_vec())
    );

    // Stopping closes the connection on the PC side too.
    let phone_id = phone.device_id();
    phone.stop();
    wait_until("the PC to see the phone leave", || {
        let status = pc.runtime.block_on(pc.daemon.status());
        status.devices.iter().all(|d| !d.connected)
    });

    // The app restarts: same identity, same pairing.
    drop(phone);
    let again = PhoneClient::new(path_of(&phone_dir), "POCO M8 5G".into()).unwrap();
    assert_eq!(again.device_id(), phone_id);
    assert_eq!(again.paired_computers(), vec![computer]);
}

#[test]
fn forgetting_a_computer_disconnects_it() {
    let pc = start_pc();
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();
    let computer = phone.pair(qr_code).unwrap();
    phone.start(Arc::new(Recorder::default()));
    wait_until("the connection", || phone.connected_count() == 1);

    phone.forget(computer.device_id.clone()).unwrap();

    wait_until("the disconnection", || phone.connected_count() == 0);
    assert!(phone.paired_computers().is_empty());
    assert!(matches!(
        phone.forget(computer.device_id),
        Err(PastegetError::UnknownDevice)
    ));
}

/// The phone forgot the PC while not connected to it, so the PC still knows it: scanning a new
/// code pairs them again.
#[test]
fn a_phone_that_forgot_the_pc_pairs_again() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");
    let pc_id = pc.daemon.device_id().to_string();
    phone.client.stop();
    wait_until("the PC to see the phone leave", || {
        let status = pc.runtime.block_on(pc.daemon.status());
        status.devices.iter().all(|d| !d.connected)
    });
    phone.client.forget(pc_id.clone()).unwrap();
    assert_eq!(pc.runtime.block_on(pc.daemon.status()).devices.len(), 1);
    phone.client.start(phone.listener.clone());

    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let computer = phone.client.pair(qr_code).unwrap();

    assert_eq!(computer.device_id, pc_id);
    assert_eq!(phone.client.paired_computers(), [computer]);
    wait_until("the new connection", || phone.client.connected_count() == 1);
    let status = pc.runtime.block_on(pc.daemon.status());
    assert_eq!(status.devices.len(), 1);
    assert!(!status.pairing_open);
    assert_eq!(phone.client.send_text("paired again".into()), 1);
    wait_until("the text on the PC clipboard", || {
        pc.clipboard.get() == Some(Raw::Text("paired again".into()))
    });
}

/// A device renamed on one side shows its new name on the other once it reconnects, and keeps it.
#[test]
fn renames_reach_the_other_side() {
    let pc = start_pc();
    let Phone {
        client, _dir: dir, ..
    } = connected_phone(&pc, "POCO");
    let pc_id = pc.daemon.device_id().to_string();

    // The phone was renamed: the app starts again with its new name.
    client.stop();
    drop(client);
    let client = PhoneClient::new(path_of(&dir), "Mon téléphone".into()).unwrap();
    client.start(Arc::new(Recorder::default()));
    wait_until("the new name on the PC", || {
        let status = pc.runtime.block_on(pc.daemon.status());
        status.devices[0].connected && status.devices[0].device.name == "Mon téléphone"
    });

    // The PC was renamed in its settings, which restarts it.
    let profile = Profile::open_in(pc._home.path(), "pc").unwrap();
    let mut settings = profile.settings().unwrap();
    settings.device_name = "Bureau".into();
    profile.save_settings(&settings).unwrap();
    let pc = restart(pc);
    assert!(client.computer_found(
        pc_id,
        vec!["127.0.0.1".into()],
        pc.daemon.local_addr().port()
    ));
    wait_until("the new name on the phone", || {
        client.paired_computers()[0].name == "Bureau"
    });

    // Both are saved.
    let known = profile.known_devices().unwrap();
    assert_eq!(known.iter().next().unwrap().name, "Mon téléphone");
    client.stop();
    drop(client);
    let again = PhoneClient::new(path_of(&dir), "Mon téléphone".into()).unwrap();
    assert_eq!(again.paired_computers()[0].name, "Bureau");
}

/// The phone forgets the PC while connected: the PC is told, and forgets the phone too.
#[test]
fn a_pc_forgotten_on_the_phone_forgets_it_too() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");
    let pc_id = pc.daemon.device_id().to_string();

    phone.client.forget(pc_id).unwrap();

    wait_until("the PC to forget the phone", || {
        pc.runtime.block_on(pc.daemon.status()).devices.is_empty()
    });
    let profile = Profile::open_in(pc._home.path(), "pc").unwrap();
    assert_eq!(profile.known_devices().unwrap().iter().count(), 0);
    // The phone doesn't dial it again, and wasn't told it was unpaired: it did that itself.
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(phone.client.connected_count(), 0);
    assert!(phone.listener.unpaired.lock().unwrap().is_empty());
    assert!(phone.client.paired_computers().is_empty());
}

/// The PC unpairs the phone: the phone hears it at once, forgets the PC and says so.
#[test]
fn a_phone_unpaired_on_the_pc_forgets_it_and_says_so() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");
    let phone_id = phone.client.device_id().parse().unwrap();

    pc.daemon.forget(&phone_id).unwrap();

    wait_until("the phone to hear it was unpaired", || {
        !phone.listener.unpaired.lock().unwrap().is_empty()
    });
    assert_eq!(phone.client.paired_computers(), []);
    assert_eq!(phone.client.connected_count(), 0);
    let name = phone.listener.unpaired.lock().unwrap()[0].clone();
    assert_eq!(name, pc.runtime.block_on(pc.daemon.status()).device_name);
}

/// What's copied while the PC is off reaches it when it's back: the last copy only.
#[test]
fn a_copy_made_while_the_pc_is_off_waits_for_it() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");
    let computer = phone.client.paired_computers()[0].clone();

    let Pc {
        runtime,
        daemon,
        _home: home,
        ..
    } = pc;
    drop(daemon);
    drop(runtime);
    wait_until("the phone to notice", || {
        phone.client.connected_count() == 0
    });
    assert_eq!(phone.client.copied_text("too old".into(), false), Some(0));
    assert_eq!(
        phone.client.copied_text("copied while off".into(), false),
        Some(0)
    );

    let pc = start_pc_in(home);
    assert!(phone.client.computer_found(
        computer.device_id,
        vec!["127.0.0.1".into()],
        pc.daemon.local_addr().port(),
    ));
    wait_until("the waiting copy on the PC", || {
        pc.clipboard.get() == Some(Raw::Text("copied while off".into()))
    });
}

/// Forwards TCP connections to the PC. It can go silent like a network that drops a device
/// without closing its connections (a phone that leaves the Wi-Fi, a PC that goes to sleep).
struct Proxy {
    address: SocketAddr,
    /// One per connection forwarded so far: fired, it stops forwarding, both ways.
    freezes: Arc<Mutex<Vec<oneshot::Sender<()>>>>,
    frozen: Frozen,
}

/// The frozen connections, still open: phone side, PC side.
type Frozen = Arc<Mutex<Vec<(Option<TcpStream>, TcpStream)>>>;

impl Proxy {
    fn start(pc: &Pc) -> Self {
        let target = pc.daemon.local_addr();
        let listener = pc
            .runtime
            .block_on(TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))
            .unwrap();
        let address = listener.local_addr().unwrap();
        let freezes = Arc::new(Mutex::new(Vec::new()));
        let frozen = Arc::new(Mutex::new(Vec::new()));
        let (accepted, held) = (Arc::clone(&freezes), Arc::clone(&frozen));
        pc.runtime.spawn(async move {
            while let Ok((phone, _)) = listener.accept().await {
                let Ok(pc) = TcpStream::connect(target).await else {
                    continue;
                };
                let (freeze, frozen) = oneshot::channel();
                accepted.lock().unwrap().push(freeze);
                tokio::spawn(forward(phone, pc, frozen, Arc::clone(&held)));
            }
        });
        Self {
            address,
            freezes,
            frozen,
        }
    }

    /// The connections open now carry nothing more, either way, and stay open.
    fn freeze(&self) {
        let frozen_before = self.frozen.lock().unwrap().len();
        let fired = self
            .freezes
            .lock()
            .unwrap()
            .drain(..)
            .filter_map(|freeze| freeze.send(()).ok())
            .count();
        wait_until("the connections to freeze", || {
            self.frozen.lock().unwrap().len() == frozen_before + fired
        });
    }

    /// The frozen connections close on the phone's side only: the PC still sees them open.
    fn hang_up_phone_side(&self) {
        for (phone, _) in self.frozen.lock().unwrap().iter_mut() {
            phone.take();
        }
    }
}

async fn forward(
    mut phone: TcpStream,
    mut pc: TcpStream,
    freeze: oneshot::Receiver<()>,
    frozen: Frozen,
) {
    {
        let (mut from_phone, mut to_phone) = phone.split();
        let (mut from_pc, mut to_pc) = pc.split();
        tokio::select! {
            _ = tokio::io::copy(&mut from_phone, &mut to_pc) => return,
            _ = tokio::io::copy(&mut from_pc, &mut to_phone) => return,
            _ = freeze => {}
        }
    }
    frozen.lock().unwrap().push((Some(phone), pc));
}

/// A session that died without closing looks alive until it times out (90 s): copies made
/// meanwhile, on either side, go into it for nothing. They are sent again once the device
/// reconnects, and both sides end with the newer one. What the old session delivered isn't
/// delivered again.
#[test]
fn copies_made_while_a_session_is_silently_dead_arrive_when_it_reconnects() {
    let pc = start_pc();
    let proxy = Proxy::start(&pc);
    let mut invite: PairingInvite = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri()
        .parse()
        .unwrap();
    invite.addresses = vec![proxy.address];
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();
    phone.pair(invite.to_uri()).unwrap();
    let listener = Arc::new(Recorder::default());
    phone.start(listener.clone());
    let connections = || listener.connections.lock().unwrap().len();
    wait_until("the connection", || connections() == 1);
    let text = |text: &str| Content {
        mime_type: mime::TEXT.into(),
        data: text.as_bytes().to_vec(),
        sensitive: false,
    };
    let received_on_the_phone = || {
        listener
            .clips
            .lock()
            .unwrap()
            .iter()
            .map(|(_, data)| String::from_utf8(data.clone()).unwrap())
            .collect::<Vec<_>>()
    };
    // Ids change each time a clip goes through, even the same one again.
    let recent_on_the_pc = || {
        pc.daemon
            .recent()
            .iter()
            .map(|clip| clip.id)
            .collect::<Vec<_>>()
    };

    // Delivered both ways, and acknowledged, while the network works.
    assert_eq!(pc.daemon.send(text("before, from the PC")), 1);
    assert_eq!(
        phone.copied_text("before, from the phone".into(), false),
        Some(1)
    );
    wait_until("both clips", || {
        received_on_the_phone().len() == 1
            && pc.clipboard.get() == Some(Raw::Text("before, from the phone".into()))
    });
    std::thread::sleep(Duration::from_millis(300));

    // The session dies silently, and the phone reconnects: nothing is sent again.
    proxy.freeze();
    proxy.hang_up_phone_side();
    let before = recent_on_the_pc();
    wait_until("the reconnection", || connections() == 3);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(received_on_the_phone(), ["before, from the PC"]);
    assert_eq!(recent_on_the_pc(), before);

    // Copies made while it's dead go into it, on both sides, for nothing: the PC's first, then
    // the phone's.
    proxy.freeze();
    assert_eq!(pc.daemon.send(text("copied on the PC meanwhile")), 1);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        phone.copied_text("copied on the phone meanwhile".into(), false),
        Some(1)
    );
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(received_on_the_phone().len(), 1);

    // Once the phone reconnects, they cross. Both sides keep the newer one, the phone's: the PC
    // takes it, the phone drops the PC's (it came before the PC's ack of the phone's).
    proxy.hang_up_phone_side();
    let delivered = || listener.deliveries.lock().unwrap().len();
    wait_until("the copies made meanwhile", || {
        delivered() == 2
            && pc.clipboard.get() == Some(Raw::Text("copied on the phone meanwhile".into()))
    });
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(received_on_the_phone(), ["before, from the PC"]);
    assert_eq!(
        pc.clipboard.get(),
        Some(Raw::Text("copied on the phone meanwhile".into()))
    );
    assert_eq!(pc.daemon.recent().len(), 4);
}

#[test]
fn a_computer_found_at_a_new_address_is_reconnected() {
    let pc = start_pc();
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();
    let computer = phone.pair(qr_code).unwrap();
    phone.start(Arc::new(Recorder::default()));
    wait_until("the connection", || phone.connected_count() == 1);
    let old_address = pc.daemon.local_addr();

    // The PC comes back somewhere else, as after its IP address changed.
    let pc = restart(pc);
    let new_address = pc.daemon.local_addr();
    assert_ne!(new_address, old_address);
    wait_until("the phone to notice", || phone.connected_count() == 0);

    // Discovery finds it there: the phone dials it right away.
    assert!(phone.computer_found(
        computer.device_id.clone(),
        vec!["127.0.0.1".into(), "not an IP".into()],
        new_address.port(),
    ));
    wait_until("the reconnection", || phone.connected_count() == 1);

    // The address that worked is remembered, first, for the next start.
    assert_eq!(
        phone.paired_computers()[0].addresses,
        [new_address.to_string(), old_address.to_string()]
    );
    let stranger = Identity::generate().unwrap().device_id().to_string();
    assert!(!phone.computer_found(stranger, vec!["127.0.0.1".into()], 1));
}

/// Roadmap M4: a screenshot reaches the PC in under a second on a LAN. Loopback is faster than
/// Wi-Fi, so this measures the software's own share: TLS, hashing, chunking and the session.
#[test]
fn a_screenshot_sized_image_is_delivered_in_under_a_second() {
    let pc = start_pc();
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();
    let computer = phone.pair(qr_code).unwrap();
    let listener = Arc::new(Recorder::default());
    phone.start(listener.clone());
    wait_until("the connection", || phone.connected_count() == 1);

    // A HyperOS screenshot is a JPEG of about 1 MB; take 2 MB. The PC acknowledges it before
    // decoding it, so its content doesn't matter here.
    let mut screenshot = vec![0xff, 0xd8, 0xff, 0xe0];
    screenshot.extend((0..2_000_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
    let size = screenshot.len() as u64;
    assert_eq!(phone.send_image(screenshot).unwrap(), 1);

    wait_until("the delivery", || {
        !listener.deliveries.lock().unwrap().is_empty()
    });
    let delivery = listener.deliveries.lock().unwrap()[0].clone();
    println!("{size} bytes delivered in {} ms", delivery.millis);
    assert_eq!(delivery.device_id, computer.device_id);
    assert_eq!(delivery.mime_type, mime::JPEG);
    assert_eq!(delivery.size, size);
    assert!(delivery.accepted);
    assert!(delivery.millis < 1000, "took {} ms", delivery.millis);
}

/// Roadmap M6: a screenshot copied on the PC reaches the phone, as a PNG the app puts on its
/// clipboard.
#[test]
fn an_image_copied_on_the_pc_reaches_the_phone() {
    let pc = start_pc();
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();
    phone.pair(qr_code).unwrap();
    let listener = Arc::new(Recorder::default());
    phone.start(listener.clone());
    wait_until("the connection", || phone.connected_count() == 1);

    // Win+Shift+S, Spectacle or grim puts the screenshot on the PC clipboard.
    let screenshot = RgbaImage {
        width: 4,
        height: 3,
        pixels: (0..4 * 3 * 4).map(|i| (i * 21) as u8).collect(),
    };
    pc.clipboard.set(Raw::Image(screenshot.clone()));

    wait_until("the image on the phone", || {
        !listener.clips.lock().unwrap().is_empty()
    });
    let (mime_type, png) = listener.clips.lock().unwrap()[0].clone();
    assert_eq!(mime_type, mime::PNG);
    let received = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    assert_eq!((received.width(), received.height()), (4, 3));
    assert_eq!(received.into_raw(), screenshot.pixels);
}

/// Roadmap M5: what the phone's clipboard watcher reports reaches the PC once, and a clip from
/// the PC is never sent back.
#[test]
fn copies_on_the_phone_reach_the_pc_without_echo() {
    let pc = start_pc();
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();
    phone.pair(qr_code).unwrap();
    let listener = Arc::new(Recorder::default());
    phone.start(listener.clone());
    wait_until("the connection", || phone.connected_count() == 1);

    assert_eq!(
        phone.copied_text("copié sur le téléphone".into(), false),
        Some(1)
    );
    wait_until("the copy on the PC clipboard", || {
        pc.clipboard.get() == Some(Raw::Text("copié sur le téléphone".into()))
    });
    // The watcher may report one copy several times: it's sent once.
    assert_eq!(
        phone.copied_text("copié sur le téléphone".into(), false),
        None
    );

    // Text copied on the PC goes on the phone clipboard, which then reports it: not sent back.
    pc.clipboard.set(Raw::Text("copied on the PC".into()));
    wait_until("the PC's copy on the phone", || {
        !listener.clips.lock().unwrap().is_empty()
    });
    assert_eq!(phone.copied_text("copied on the PC".into(), false), None);

    // A new copy on the phone is new again.
    assert_eq!(phone.copied_text("encore".into(), true), Some(1));
    wait_until("the second copy on the PC clipboard", || {
        pc.clipboard.get() == Some(Raw::Text("encore".into()))
    });
}

/// v0.2: a one-time code from a notification lands on the PC clipboard, as a secret.
#[test]
fn a_one_time_code_reaches_the_pc_clipboard_as_a_secret() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");

    assert_eq!(
        pasteget_ffi::find_one_time_code("G-482913 is your Google verification code".into()),
        Some("482913".into())
    );
    assert_eq!(
        phone
            .client
            .send_one_time_code("482913".into(), "Messages".into()),
        1
    );
    wait_until("the code on the PC clipboard", || {
        pc.clipboard.get() == Some(Raw::Text("482913".into()))
    });
    // Kept out of the tray's Recent list, like any secret.
    assert!(pc.daemon.recent().is_empty());
    // If the user then copies it on the phone, it isn't sent twice.
    assert_eq!(phone.client.copied_text("482913".into(), true), None);
}

/// Roadmap M6: text of any length copied on the PC reaches the phone whole, in many chunks.
/// The app puts texts this long on its clipboard as a file link (Binder's 1 MB limit).
#[test]
fn a_long_text_copied_on_the_pc_reaches_the_phone_whole() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");

    // 3 MB of text with characters of 1 to 4 bytes, so chunks split some of them.
    let text: String = (0..1_200_000)
        .map(|i| ["a", "é", "€", "👋"][i % 4])
        .collect();
    assert_eq!(text.len(), 3_000_000);
    pc.clipboard.set(Raw::Text(text.clone()));

    let (mime_type, data) = phone.clip(1);
    assert_eq!(mime_type, mime::TEXT);
    assert_eq!(String::from_utf8(data).unwrap(), text);
}

/// Roadmap M6: a clip the PC marks as secret stays marked on the phone, which keeps it out of
/// clipboard history.
#[test]
fn a_sensitive_clip_from_the_pc_stays_sensitive() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");

    pc.daemon.send(Content {
        mime_type: mime::TEXT.into(),
        data: b"hunter2".to_vec(),
        sensitive: true,
    });

    assert_eq!(phone.clip(1), (mime::TEXT.to_owned(), b"hunter2".to_vec()));
    assert_eq!(*phone.listener.sensitive.lock().unwrap(), [true]);
}

/// Roadmap M6, done when: a full-HD screenshot copied on the PC reaches the phone as a PNG
/// with the same pixels.
#[test]
fn a_full_hd_screenshot_copied_on_the_pc_reaches_the_phone() {
    let pc = start_pc();
    let phone = connected_phone(&pc, "POCO");
    let (width, height) = (1920u32, 1080u32);
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|i| {
            let (x, y) = (i % width, i / width);
            [(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]
        })
        .collect();
    let started = Instant::now();

    pc.clipboard.set(Raw::Image(RgbaImage {
        width,
        height,
        pixels: pixels.clone(),
    }));

    let (mime_type, png) = phone.clip(1);
    println!(
        "{width}x{height} screenshot: {} bytes of PNG on the phone in {} ms (debug build)",
        png.len(),
        started.elapsed().as_millis()
    );
    assert_eq!(mime_type, mime::PNG);
    let received = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    assert_eq!((received.width(), received.height()), (width, height));
    assert!(received.into_raw() == pixels, "the pixels differ");
}

/// A JPEG photo shared from one phone reaches the other phones through the PC, as it is.
#[test]
fn a_jpeg_from_one_phone_reaches_another_through_the_pc() {
    let pc = start_pc();
    let first = connected_phone(&pc, "POCO");
    let second = connected_phone(&pc, "Tablet");
    let photo = jpeg(64, 48);

    assert_eq!(first.client.send_image(photo.clone()).unwrap(), 1);

    assert_eq!(second.clip(1), (mime::JPEG.to_owned(), photo));
    // Not back to the phone it came from.
    std::thread::sleep(Duration::from_millis(200));
    assert!(first.listener.clips.lock().unwrap().is_empty());
}

/// The error names every address of the code, with its own reason.
#[test]
fn pairing_says_why_each_address_failed() {
    let pc = start_pc();
    let other_pc = start_pc();
    let mut invite: PairingInvite = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri()
        .parse()
        .unwrap();
    let closed = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap();
    let other = other_pc.daemon.local_addr();
    invite.addresses = vec![closed, other];
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();

    let error = phone.pair(invite.to_uri()).unwrap_err();
    assert!(matches!(error, PastegetError::Unreachable(_)));
    assert_eq!(
        error.to_string(),
        format!(
            "could not reach the computer: {closed}: connection refused; \
             {other}: a different device answered"
        )
    );

    // With the PC's own address too, it pairs, and that address is dialed first.
    let address = pc.daemon.local_addr();
    invite.addresses.push(address);
    let computer = phone.pair(invite.to_uri()).unwrap();
    assert_eq!(
        computer.addresses,
        [address, closed, other].map(|a| a.to_string())
    );
}

/// A saved list of computers that can't be read is set aside: the app starts with none.
#[test]
fn an_unreadable_computer_list_is_set_aside() {
    let pc = start_pc();
    let phone_dir = tempfile::tempdir().unwrap();
    let saved = phone_dir.path().join("computers.json");
    std::fs::write(&saved, b"[{\"device_id\": \"ABC\", \"na").unwrap();

    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();

    assert!(phone.paired_computers().is_empty());
    assert_eq!(
        std::fs::read(phone_dir.path().join("computers.json.bad")).unwrap(),
        b"[{\"device_id\": \"ABC\", \"na"
    );
    // Pairing again works, and is saved.
    let qr_code = pc
        .runtime
        .block_on(pc.daemon.open_pairing())
        .unwrap()
        .to_uri();
    let computer = phone.pair(qr_code).unwrap();
    drop(phone);
    let again = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();
    assert_eq!(again.paired_computers(), [computer]);
}

#[test]
fn bad_input_is_refused() {
    let phone_dir = tempfile::tempdir().unwrap();
    let phone = PhoneClient::new(path_of(&phone_dir), "POCO".into()).unwrap();

    assert!(matches!(
        phone.pair("https://example.com".into()),
        Err(PastegetError::InvalidCode)
    ));
    assert!(matches!(
        phone.send_image(b"GIF89a".to_vec()),
        Err(PastegetError::UnsupportedImage)
    ));
    assert_eq!(phone.send_text("nobody listening".into()), 0);
}
