// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Application, Element, Theme,
    app::{Core, Task},
    cosmic_config::{self, CosmicConfigEntry},
    iced::{Subscription, window::Id},
    iced_runtime::core::window,
    surface,
    surface::action::{app_popup, destroy_popup},
    widget::container,
};
use std::time::Duration;
use sysinfo::{
    Components, CpuRefreshKind, Disk, DiskRefreshKind, Disks, MemoryRefreshKind, Networks,
    RefreshKind, System,
};

use crate::{
    components::gpu::Gpus,
    config::{
        ComponentConfig, Config, IconColorMode, QuickIntervalProfile, Sampling, SamplingConfig,
        TextFormatMode, config_subscription,
    },
    history::History,
};

pub const ID: &str = "dev.DBrox.CosmicSystemMonitor";

pub struct SystemMonitorApplet {
    pub core: Core,
    pub config: Config,
    #[allow(dead_code)]
    config_handler: Option<cosmic_config::Config>,
    pub popup: Option<Id>,

    pub sys: System,
    pub nets: Networks,
    pub disks: Disks,
    pub gpus: Gpus,
    pub components: Components,
    pub cpu_temperature: Option<f32>,
    /// percentage global cpu used between refreshes
    pub global_cpu: History<f32>,
    pub ram: History,
    pub swap: History,
    /// amount uploaded between refresh of `sysinfo::Nets`. (DOES NOT STORE RATE)
    pub upload: History,
    /// amount downloaded between refresh of `sysinfo::Nets`. (DOES NOT STORE RATE)
    pub download: History,
    /// amount read between refresh of `sysinfo::Disks`. (DOES NOT STORE RATE)
    pub disk_read: History,
    /// amount written between refresh of `sysinfo::Disks`. (DOES NOT STORE RATE)
    pub disk_write: History,
    /// amount read between refresh of `sysinfo::Disks`. (DOES NOT STORE RATE)
    pub gpu_usage: Vec<History>,
    /// amount written between refresh of `sysinfo::Disks`. (DOES NOT STORE RATE)
    pub vram: Vec<History>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Config(Config),
    PopupClosed(Id),
    TogglePopup,
    ToggleModuleCpu(bool),
    ToggleModuleMem(bool),
    ToggleModuleNet(bool),
    ToggleModuleDisk(bool),
    ToggleModuleGpu(bool),
    ToggleCpuTemperature(bool),
    SetTextFormatMode(TextFormatMode),
    SetIconColorMode(IconColorMode),
    SetQuickIntervalProfile(QuickIntervalProfile),
    TickCpu,
    TickMem,
    TickNet,
    TickDisk,
    TickGpu,
    Surface(surface::Action),
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: Config,
}

impl Application for SystemMonitorApplet {
    type Executor = cosmic::executor::Default;

    type Flags = Flags;

    type Message = Message;

    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let (mut cpu, mut mem, mut net, mut disk, mut gpu) = Default::default();
        let sampling = &flags.config.sampling;
        for chart_config in &flags.config.components {
            match chart_config {
                ComponentConfig::Cpu(_) => cpu = sampling.cpu.sampling_window,
                ComponentConfig::Mem(_) => mem = sampling.mem.sampling_window,
                ComponentConfig::Net(_) => net = sampling.net.sampling_window,
                ComponentConfig::Disk(_) => disk = sampling.disk.sampling_window,
                ComponentConfig::Gpu(_) => gpu = sampling.gpu.sampling_window,
            }
        }
        let gpus = Gpus::new();
        let app = Self {
            core,
            config: flags.config,
            config_handler: flags.config_handler,
            popup: None,

            global_cpu: History::with_capacity(cpu),
            ram: History::with_capacity(mem),
            swap: History::with_capacity(mem),
            upload: History::with_capacity(net),
            download: History::with_capacity(net),
            disk_read: History::with_capacity(disk),
            disk_write: History::with_capacity(disk),
            gpu_usage: vec![History::with_capacity(gpu); gpus.num_gpus()],
            vram: vec![History::with_capacity(gpu); gpus.num_gpus()],

            sys: System::new_with_specifics(
                RefreshKind::nothing()
                    .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                    .with_memory(MemoryRefreshKind::everything()),
            ),
            nets: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list_specifics(
                DiskRefreshKind::nothing().with_io_usage(),
            ),
            gpus,
            components: Components::new_with_refreshed_list(),
            cpu_temperature: None,
        };

        (app, Task::none())
    }

    fn view(&'_ self) -> Element<'_, Message> {
        let items = self.main_content();
        self.core
            .applet
            .autosize_window(self.main_button(items))
            .into()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Config(config) => {
                self.config = config;
                self.resize_histories();
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(destroy_popup(id)),
                    ));
                }

                return cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(
                    app_popup::<SystemMonitorApplet>(
                        move |state: &mut SystemMonitorApplet| {
                            let new_id = Id::unique();
                            state.popup = Some(new_id);
                            let mut popup_settings = state.core.applet.get_popup_settings(
                                state.core.main_window_id().unwrap_or(Id::NONE),
                                new_id,
                                None,
                                None,
                                None,
                            );
                            popup_settings.positioner.reactive = true;
                            popup_settings
                        },
                        Some(Box::new(|state: &SystemMonitorApplet| {
                            state.settings_popup_view().map(cosmic::Action::App)
                        })),
                    ),
                )));
            }
            Message::ToggleModuleCpu(value) => {
                self.config.ui.enabled_modules.cpu = value;
                self.persist_config();
            }
            Message::ToggleModuleMem(value) => {
                self.config.ui.enabled_modules.mem = value;
                self.persist_config();
            }
            Message::ToggleModuleNet(value) => {
                self.config.ui.enabled_modules.net = value;
                self.persist_config();
            }
            Message::ToggleModuleDisk(value) => {
                self.config.ui.enabled_modules.disk = value;
                self.persist_config();
            }
            Message::ToggleModuleGpu(value) => {
                self.config.ui.enabled_modules.gpu = value;
                self.persist_config();
            }
            Message::ToggleCpuTemperature(value) => {
                self.config.ui.show_cpu_temperature = value;
                self.persist_config();
            }
            Message::SetTextFormatMode(value) => {
                self.config.ui.text_format_mode = value;
                self.persist_config();
            }
            Message::SetIconColorMode(value) => {
                self.config.ui.icon_color_mode = value;
                self.persist_config();
            }
            Message::SetQuickIntervalProfile(value) => {
                self.config.ui.quick_interval_profile = value;
                self.config.sampling = sampling_config_for_profile(value);
                self.resize_histories();
                self.persist_config();
            }
            Message::TickCpu => {
                self.sys.refresh_cpu_usage();
                self.global_cpu.push(self.sys.global_cpu_usage());
                if self.config.ui.show_cpu_temperature {
                    self.components.refresh(false);
                    self.cpu_temperature = detect_cpu_temperature(&self.components);
                } else {
                    self.cpu_temperature = None;
                }
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Message::TickMem => {
                self.sys.refresh_memory();
                self.ram.push(self.sys.used_memory());
                self.swap.push(self.sys.used_swap());
            }
            Message::TickNet => {
                self.nets.refresh(true);
                let (received, transmitted) =
                    self.nets.iter().fold((0, 0), |(acc_r, acc_t), (_, data)| {
                        (acc_r + data.received(), acc_t + data.transmitted())
                    });
                self.upload.push(transmitted);
                self.download.push(received);
            }
            Message::TickDisk => {
                self.disks
                    .refresh_specifics(true, DiskRefreshKind::nothing().with_io_usage());
                let (read, written) = self
                    .disks
                    .iter()
                    .map(Disk::usage)
                    .fold((0, 0), |(acc_r, acc_w), usage| {
                        (acc_r + usage.read_bytes, acc_w + usage.written_bytes)
                    });
                self.disk_read.push(read);
                self.disk_write.push(written);
            }
            Message::TickGpu => {
                self.gpus.refresh();
                for (idx, data) in self.gpus.data().iter().enumerate() {
                    self.gpu_usage[idx].push(data.usage);
                    self.vram[idx].push(data.used_vram);
                }
            }
        }
        Task::none()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        cosmic::widget::text("").into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subs = Vec::new();
        let sampling = &self.config.sampling;
        let enabled = self.config.ui.enabled_modules;
        let mut wants_cpu = false;
        let mut wants_mem = false;
        let mut wants_net = false;
        let mut wants_disk = false;
        let mut wants_gpu = false;

        for chart in &self.config.components {
            match chart {
                ComponentConfig::Cpu(_) if enabled.cpu => wants_cpu = true,
                ComponentConfig::Mem(_) if enabled.mem => wants_mem = true,
                ComponentConfig::Net(_) if enabled.net => wants_net = true,
                ComponentConfig::Disk(_) if enabled.disk => wants_disk = true,
                ComponentConfig::Gpu(_) if enabled.gpu => wants_gpu = true,
                _ => {}
            }
        }

        if wants_cpu {
            subs.push(
                cosmic::iced::time::every(Duration::from_millis(sampling.cpu.update_interval))
                    .map(|_| Message::TickCpu),
            );
        }
        if wants_mem {
            subs.push(
                cosmic::iced::time::every(Duration::from_millis(sampling.mem.update_interval))
                    .map(|_| Message::TickMem),
            );
        }
        if wants_net {
            subs.push(
                cosmic::iced::time::every(Duration::from_millis(sampling.net.update_interval))
                    .map(|_| Message::TickNet),
            );
        }
        if wants_disk {
            subs.push(
                cosmic::iced::time::every(Duration::from_millis(sampling.disk.update_interval))
                    .map(|_| Message::TickDisk),
            );
        }
        if wants_gpu {
            subs.push(
                cosmic::iced::time::every(Duration::from_millis(sampling.gpu.update_interval))
                    .map(|_| Message::TickGpu),
            );
        }

        subs.push(config_subscription());

        Subscription::batch(subs)
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}

impl SystemMonitorApplet {
    fn persist_config(&self) {
        if let Some(handler) = &self.config_handler {
            if let Err(err) = self.config.write_entry(handler) {
                eprintln!("failed to persist config: {err}");
            }
        }
    }

    fn resize_histories(&mut self) {
        let sampling = &self.config.sampling;
        self.global_cpu.resize(sampling.cpu.sampling_window);
        self.ram.resize(sampling.mem.sampling_window);
        self.swap.resize(sampling.mem.sampling_window);
        self.upload.resize(sampling.net.sampling_window);
        self.download.resize(sampling.net.sampling_window);
        self.disk_read.resize(sampling.disk.sampling_window);
        self.disk_write.resize(sampling.disk.sampling_window);
        for i in 0..self.gpus.num_gpus() {
            self.gpu_usage[i].resize(sampling.gpu.sampling_window);
            self.vram[i].resize(sampling.gpu.sampling_window);
        }
    }
}

pub fn base_background(theme: &Theme) -> container::Style {
    let on_primary = cosmic::iced::Color::from(theme.cosmic().primary.on);
    let mut base_color = cosmic::iced::Color::from(theme.cosmic().primary.base);
    base_color.a *= 0.5;
    container::Style {
        background: Some(base_color.into()),
        icon_color: Some(cosmic::iced::Color::WHITE),
        text_color: Some(on_primary),
        ..container::Style::default()
    }
}

fn detect_cpu_temperature(components: &Components) -> Option<f32> {
    let mut total = 0.0f32;
    let mut count = 0u32;
    for component in components.iter() {
        let label = component.label().to_ascii_lowercase();
        let is_cpu_sensor = label.contains("cpu")
            || label.contains("package")
            || label.contains("tctl")
            || label.contains("tdie")
            || label.starts_with("core ");
        if is_cpu_sensor {
            if let Some(temp) = component.temperature() {
                if temp.is_finite() {
                    total += temp;
                    count += 1;
                }
            }
        }
    }

    if count == 0 {
        for component in components.iter() {
            if let Some(temp) = component.temperature() {
                if temp.is_finite() {
                    total += temp;
                    count += 1;
                }
            }
        }
    }

    if count == 0 {
        None
    } else {
        Some(total / count as f32)
    }
}

fn sampling_config_for_profile(profile: QuickIntervalProfile) -> SamplingConfig {
    let interval = match profile {
        QuickIntervalProfile::Fast => 1000,
        QuickIntervalProfile::Normal => 5000,
        QuickIntervalProfile::Slow => 10000,
    };
    SamplingConfig {
        cpu: Sampling {
            update_interval: interval,
            sampling_window: 60,
        },
        mem: Sampling {
            update_interval: interval,
            sampling_window: 30,
        },
        net: Sampling {
            update_interval: interval,
            sampling_window: 30,
        },
        disk: Sampling {
            update_interval: interval,
            sampling_window: 60,
        },
        gpu: Sampling {
            update_interval: interval,
            sampling_window: 30,
        },
    }
}
