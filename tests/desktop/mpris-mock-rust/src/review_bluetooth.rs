//! Synthetic devices on the review session bus; never a host Bluetooth adapter.
use zbus::{interface, zvariant::ObjectPath};

pub struct Adapter { pub powered: bool }
#[interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)] fn address(&self)->&str { "02:00:00:00:00:01" }
    #[zbus(property)] fn address_type(&self)->&str { "public" }
    #[zbus(property)] fn name(&self)->&str { "Studio Desktop" }
    #[zbus(property)] fn alias(&self)->&str { "Studio Desktop" }
    #[zbus(property)] fn class(&self)->u32 { 0x104 }
    #[zbus(property)] fn powered(&self)->bool { self.powered }
    #[zbus(property)] fn set_powered(&mut self,value:bool) { self.powered=value; }
    #[zbus(property)] fn discovering(&self)->bool { false }
    #[zbus(property)] fn discoverable(&self)->bool { false }
    #[zbus(property)] fn pairable(&self)->bool { true }
    #[zbus(property)] fn uuids(&self)->Vec<String> { vec![] }
    fn start_discovery(&self) {}
    fn stop_discovery(&self) {}
}

pub struct Headphones;
#[interface(name = "org.bluez.Device1")]
impl Headphones {
    #[zbus(property)] fn adapter(&self)->ObjectPath<'static> { ObjectPath::from_static_str_unchecked("/org/bluez/hci0") }
    #[zbus(property)] fn address(&self)->&str { "02:00:00:00:00:02" }
    #[zbus(property)] fn address_type(&self)->&str { "public" }
    #[zbus(property)] fn name(&self)->&str { "Studio Headphones" }
    #[zbus(property)] fn alias(&self)->&str { "Studio Headphones" }
    #[zbus(property)] fn icon(&self)->&str { "audio-headphones" }
    #[zbus(property)] fn paired(&self)->bool { true }
    #[zbus(property)] fn bonded(&self)->bool { true }
    #[zbus(property)] fn connected(&self)->bool { true }
    #[zbus(property)] fn trusted(&self)->bool { true }
    #[zbus(property)] fn blocked(&self)->bool { false }
    #[zbus(property)] fn services_resolved(&self)->bool { true }
    #[zbus(property)] fn rssi(&self)->i16 { -42 }
    #[zbus(property)] fn class(&self)->u32 { 0x240404 }
    #[zbus(property)] fn uuids(&self)->Vec<String> { vec!["0000110b-0000-1000-8000-00805f9b34fb".into()] }
}

pub struct Battery;
#[interface(name = "org.bluez.Battery1")]
impl Battery { #[zbus(property)] fn percentage(&self)->u8 { 78 } }
