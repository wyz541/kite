#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub volumes: Vec<StorageVolume>,
    pub portable_devices: Vec<PortableDevice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    Usb,
    Removable,
    Fixed,
    Optical,
    Network,
    Other,
}

impl VolumeKind {
    #[allow(dead_code)]
    pub fn is_removable(self) -> bool {
        matches!(self, Self::Usb | Self::Removable)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageVolume {
    pub path: std::path::PathBuf,
    pub volume_name: Option<String>,
    pub device_name: Option<String>,
    pub kind: VolumeKind,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortableDevice {
    pub name: String,
    pub shell_path: String,
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{DeviceSnapshot, PortableDevice, StorageVolume, VolumeKind};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::path::PathBuf;
    use std::ptr;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Device_IDW, DIGCF_PRESENT, GUID_DEVCLASS_WPD, SP_DEVINFO_DATA, SPDRP_DEVICEDESC,
        SPDRP_FRIENDLYNAME, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
        SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BusTypeUsb, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
        GetVolumeNameForVolumeMountPointW, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR,
        STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
    };

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;

    #[derive(Default)]
    struct StorageDetails {
        bus_type: Option<i32>,
        vendor: Option<String>,
        product: Option<String>,
    }

    pub fn snapshot() -> DeviceSnapshot {
        DeviceSnapshot {
            volumes: storage_volumes(),
            portable_devices: portable_devices(),
        }
    }

    fn storage_volumes() -> Vec<StorageVolume> {
        let mask = unsafe { GetLogicalDrives() };
        let mut volumes = Vec::new();

        for index in 0..26 {
            if mask & (1 << index) == 0 {
                continue;
            }
            let letter = (b'A' + index as u8) as char;
            let root = format!("{letter}:\\");
            let root_wide = wide(&root);
            let drive_type = unsafe { GetDriveTypeW(root_wide.as_ptr()) };
            if drive_type <= 1 {
                continue;
            }

            let details = storage_details(&root_wide);
            let kind = classify_volume(drive_type, details.bus_type);
            let (free_bytes, total_bytes) = volume_space(&root_wide);
            let device_name = combine_device_name(details.vendor, details.product);

            volumes.push(StorageVolume {
                path: PathBuf::from(root),
                volume_name: volume_label(&root_wide),
                device_name,
                kind,
                free_bytes,
                total_bytes,
            });
        }

        volumes
    }

    fn classify_volume(drive_type: u32, bus_type: Option<i32>) -> VolumeKind {
        match drive_type {
            DRIVE_REMOVABLE if bus_type == Some(BusTypeUsb) => VolumeKind::Usb,
            DRIVE_REMOVABLE => VolumeKind::Removable,
            DRIVE_FIXED if bus_type == Some(BusTypeUsb) => VolumeKind::Usb,
            DRIVE_FIXED => VolumeKind::Fixed,
            DRIVE_REMOTE => VolumeKind::Network,
            DRIVE_CDROM => VolumeKind::Optical,
            _ => VolumeKind::Other,
        }
    }

    fn storage_details(root: &[u16]) -> StorageDetails {
        let mut volume_name = [0u16; 64];
        if unsafe {
            GetVolumeNameForVolumeMountPointW(
                root.as_ptr(),
                volume_name.as_mut_ptr(),
                volume_name.len() as u32,
            )
        } == 0
        {
            return StorageDetails::default();
        }

        let handle = unsafe {
            CreateFileW(
                volume_name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return StorageDetails::default();
        }

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut buffer = [0u64; 128];
        let mut returned = 0u32;
        let result = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&query as *const STORAGE_PROPERTY_QUERY).cast::<c_void>(),
                size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                buffer.as_mut_ptr().cast::<c_void>(),
                size_of_val(&buffer) as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        unsafe {
            CloseHandle(handle);
        }

        if result == 0 || returned < size_of::<STORAGE_DEVICE_DESCRIPTOR>() as u32 {
            return StorageDetails::default();
        }

        let bytes = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().cast::<u8>(),
                returned.min(size_of_val(&buffer) as u32) as usize,
            )
        };
        let descriptor = unsafe { &*(bytes.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>()) };

        StorageDetails {
            bus_type: Some(descriptor.BusType),
            vendor: read_ascii(bytes, descriptor.VendorIdOffset),
            product: read_ascii(bytes, descriptor.ProductIdOffset),
        }
    }

    fn combine_device_name(vendor: Option<String>, product: Option<String>) -> Option<String> {
        let vendor = vendor.filter(|value| !value.is_empty());
        let product = product.filter(|value| !value.is_empty());
        match (vendor, product) {
            (Some(vendor), Some(product)) if !vendor.eq_ignore_ascii_case(&product) => {
                Some(format!("{vendor} {product}"))
            }
            (Some(value), _) | (_, Some(value)) => Some(value),
            _ => None,
        }
    }

    fn read_ascii(bytes: &[u8], offset: u32) -> Option<String> {
        let start = offset as usize;
        if start == 0 || start >= bytes.len() {
            return None;
        }
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|position| start + position)
            .unwrap_or(bytes.len());
        let value = String::from_utf8_lossy(&bytes[start..end])
            .trim()
            .to_string();
        (!value.is_empty()).then_some(value)
    }

    fn volume_label(root: &[u16]) -> Option<String> {
        let mut name = [0u16; 261];
        let result = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            )
        };
        if result == 0 {
            return None;
        }
        let value = utf16_string(&name);
        (!value.is_empty()).then_some(value)
    }

    fn volume_space(root: &[u16]) -> (Option<u64>, Option<u64>) {
        let mut free = 0u64;
        let mut total = 0u64;
        let result =
            unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut free, &mut total, ptr::null_mut()) };
        if result == 0 {
            (None, None)
        } else {
            (Some(free), Some(total))
        }
    }

    fn portable_devices() -> Vec<PortableDevice> {
        let devices = unsafe {
            SetupDiGetClassDevsW(
                &GUID_DEVCLASS_WPD,
                ptr::null(),
                ptr::null_mut(),
                DIGCF_PRESENT,
            )
        };
        if devices == -1 {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut index = 0u32;
        loop {
            let mut info = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if unsafe { SetupDiEnumDeviceInfo(devices, index, &mut info) } == 0 {
                break;
            }

            let name = device_registry_string(devices, &info, SPDRP_FRIENDLYNAME)
                .or_else(|| device_registry_string(devices, &info, SPDRP_DEVICEDESC));
            let instance_id = device_instance_id(&info);
            if let (Some(name), Some(instance_id)) = (name, instance_id)
                && !name.is_empty()
            {
                let shell_path = portable_shell_path(&instance_id);
                if !result
                    .iter()
                    .any(|device: &PortableDevice| device.shell_path == shell_path)
                {
                    result.push(PortableDevice { name, shell_path });
                }
            }
            index += 1;
        }

        unsafe {
            SetupDiDestroyDeviceInfoList(devices);
        }
        result.sort_by(|left, right| left.name.cmp(&right.name));
        result
    }

    fn device_instance_id(info: &SP_DEVINFO_DATA) -> Option<String> {
        let mut buffer = [0u16; 512];
        let result =
            unsafe { CM_Get_Device_IDW(info.DevInst, buffer.as_mut_ptr(), buffer.len() as u32, 0) };
        (result == 0)
            .then(|| utf16_string(&buffer))
            .filter(|value| !value.is_empty())
    }

    fn portable_shell_path(instance_id: &str) -> String {
        const COMPUTER_CLSID: &str = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\\";
        const WPD_DEVICE_INTERFACE: &str = "{6ac27878-a6fa-4155-ba85-f98f491d4f33}";
        let escaped = instance_id.to_ascii_lowercase().replace('\\', "#");
        format!("{COMPUTER_CLSID}\\\\?\\{escaped}#{WPD_DEVICE_INTERFACE}")
    }

    fn device_registry_string(
        devices: isize,
        info: &SP_DEVINFO_DATA,
        property: u32,
    ) -> Option<String> {
        let mut buffer = [0u16; 512];
        let mut required = 0u32;
        let mut property_type = 0u32;
        let result = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                devices,
                info,
                property,
                &mut property_type,
                buffer.as_mut_ptr().cast::<u8>(),
                size_of_val(&buffer) as u32,
                &mut required,
            )
        };
        (result != 0)
            .then(|| utf16_string(&buffer))
            .filter(|name| !name.is_empty())
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn utf16_string(buffer: &[u16]) -> String {
        let length = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..length])
            .trim()
            .to_string()
    }

    #[cfg(test)]
    mod tests {
        use super::super::VolumeKind;
        use super::{
            DRIVE_FIXED, DRIVE_REMOVABLE, classify_volume, portable_shell_path, read_ascii,
        };
        use windows_sys::Win32::Storage::FileSystem::BusTypeUsb;

        #[test]
        fn classifies_usb_volumes_by_bus_type() {
            assert_eq!(
                classify_volume(DRIVE_FIXED, Some(BusTypeUsb)),
                VolumeKind::Usb
            );
            assert_eq!(
                classify_volume(DRIVE_REMOVABLE, Some(BusTypeUsb)),
                VolumeKind::Usb
            );
            assert_eq!(classify_volume(DRIVE_FIXED, None), VolumeKind::Fixed);
            assert_eq!(
                classify_volume(DRIVE_REMOVABLE, None),
                VolumeKind::Removable
            );
        }

        #[test]
        fn reads_null_terminated_storage_names() {
            let bytes = b"\0\0Kingston\0DataTraveler\0";
            assert_eq!(read_ascii(bytes, 2), Some("Kingston".to_string()));
            assert_eq!(read_ascii(bytes, 11), Some("DataTraveler".to_string()));
        }

        #[test]
        fn builds_the_wpd_shell_namespace_path() {
            assert_eq!(
                portable_shell_path(r"USB\VID_05AC&PID_12A8&MI_00\6&2E02EB0C&0&0000"),
                r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\\\?\usb#vid_05ac&pid_12a8&mi_00#6&2e02eb0c&0&0000#{6ac27878-a6fa-4155-ba85-f98f491d4f33}"
            );
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{DeviceSnapshot, StorageVolume, VolumeKind};
    use sysinfo::Disks;

    pub fn snapshot() -> DeviceSnapshot {
        let disks = Disks::new_with_refreshed_list();
        let mut volumes = disks
            .list()
            .iter()
            .map(|disk| {
                let mount = disk.mount_point();
                let file_system = disk.file_system().to_string_lossy().to_lowercase();
                let kind = if matches!(file_system.as_str(), "smbfs" | "nfs" | "afpfs") {
                    VolumeKind::Network
                } else if matches!(file_system.as_str(), "cd9660" | "udf") {
                    VolumeKind::Optical
                } else if disk.is_removable() || mount.starts_with("/Volumes/") {
                    VolumeKind::Removable
                } else {
                    VolumeKind::Fixed
                };
                let name = disk.name().to_string_lossy().trim().to_string();
                StorageVolume {
                    path: mount.to_path_buf(),
                    volume_name: (!name.is_empty()).then_some(name.clone()),
                    device_name: (!name.is_empty()).then_some(name),
                    kind,
                    free_bytes: Some(disk.available_space()),
                    total_bytes: Some(disk.total_space()),
                }
            })
            .collect::<Vec<_>>();
        volumes.sort_by(|left, right| left.path.cmp(&right.path));
        DeviceSnapshot {
            volumes,
            portable_devices: Vec::new(),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::DeviceSnapshot;

    pub fn snapshot() -> DeviceSnapshot {
        DeviceSnapshot {
            volumes: Vec::new(),
            portable_devices: Vec::new(),
        }
    }
}

pub use platform::snapshot;
