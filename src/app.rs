use std::collections::HashMap;

use crate::model::{NetworkEvent, NetworkInterface, NetworkSnapshot};

#[derive(Clone, Debug, Default)]
pub struct App {
    pub current_snapshot: Option<NetworkSnapshot>,
    pub previous_snapshot: Option<NetworkSnapshot>,
    pub selected_index: usize,
    pub recent_events: Vec<NetworkEvent>,
}

impl App {
    pub fn replace_snapshot(&mut self, snapshot: NetworkSnapshot) {
        let selected_name = self.selected_interface_name().map(str::to_owned);

        if let Some(previous) = self.current_snapshot.replace(snapshot) {
            self.previous_snapshot = Some(previous);
        }

        self.push_generated_events();
        self.restore_selection(selected_name.as_deref());
    }

    pub fn selected_interface_name(&self) -> Option<&str> {
        self.current_snapshot
            .as_ref()?
            .interfaces
            .get(self.selected_index)
            .map(|interface| interface.name.as_str())
    }

    pub fn selected_rates(&self) -> Option<(u64, u64)> {
        let current = self.current_snapshot.as_ref()?;
        let previous = self.previous_snapshot.as_ref()?;
        let elapsed = current
            .captured_at_secs
            .checked_sub(previous.captured_at_secs)?;

        if elapsed == 0 {
            return None;
        }

        let selected = current.interfaces.get(self.selected_index)?;
        let previous_interface = previous
            .interfaces
            .iter()
            .find(|interface| interface.name == selected.name)?;
        let current_stats = selected.stats.as_ref()?;
        let previous_stats = previous_interface.stats.as_ref()?;

        Some((
            current_stats
                .rx_bytes
                .saturating_sub(previous_stats.rx_bytes)
                / elapsed,
            current_stats
                .tx_bytes
                .saturating_sub(previous_stats.tx_bytes)
                / elapsed,
        ))
    }

    fn push_generated_events(&mut self) {
        let Some(current) = self.current_snapshot.as_ref() else {
            return;
        };

        let mut new_events = Vec::new();

        if let Some(previous) = self.previous_snapshot.as_ref() {
            let previous_by_name = interfaces_by_name(&previous.interfaces);
            let current_by_name = interfaces_by_name(&current.interfaces);

            for interface in &current.interfaces {
                match previous_by_name.get(interface.name.as_str()) {
                    None => new_events.push(NetworkEvent::new(
                        format!("{} appeared", interface.name),
                        current.captured_at_secs,
                    )),
                    Some(previous_interface) => {
                        if previous_interface.status != interface.status {
                            new_events.push(NetworkEvent::new(
                                format!(
                                    "{} status changed: {} -> {}",
                                    interface.name,
                                    status_label(&previous_interface.status),
                                    status_label(&interface.status)
                                ),
                                current.captured_at_secs,
                            ));
                        }

                        let before = first_ipv4(previous_interface);
                        let after = first_ipv4(interface);

                        if before != after {
                            if let (Some(before), Some(after)) = (before, after) {
                                new_events.push(NetworkEvent::new(
                                    format!("{} IPv4 changed: {} -> {}", interface.name, before, after),
                                    current.captured_at_secs,
                                ));
                            }
                        }
                    }
                }
            }

            for interface in &previous.interfaces {
                if !current_by_name.contains_key(interface.name.as_str()) {
                    new_events.push(NetworkEvent::new(
                        format!("{} disappeared", interface.name),
                        current.captured_at_secs,
                    ));
                }
            }
        }

        self.recent_events.extend(new_events);

        if self.recent_events.len() > 50 {
            let overflow = self.recent_events.len() - 50;
            self.recent_events.drain(0..overflow);
        }
    }

    fn restore_selection(&mut self, selected_name: Option<&str>) {
        let Some(current) = self.current_snapshot.as_ref() else {
            self.selected_index = 0;
            return;
        };

        let len = current.interfaces.len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }

        if let Some(selected_name) = selected_name {
            if let Some(index) = current
                .interfaces
                .iter()
                .position(|interface| interface.name == selected_name)
            {
                self.selected_index = index;
                return;
            }
        }

        if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }
}

fn interfaces_by_name<'a>(
    interfaces: &'a [NetworkInterface],
) -> HashMap<&'a str, &'a NetworkInterface> {
    interfaces
        .iter()
        .map(|interface| (interface.name.as_str(), interface))
        .collect()
}

fn first_ipv4(interface: &NetworkInterface) -> Option<&str> {
    interface.ipv4.first().map(|address| address.value.as_str())
}

fn status_label(status: &crate::model::InterfaceStatus) -> &'static str {
    match status {
        crate::model::InterfaceStatus::Up => "up",
        crate::model::InterfaceStatus::Down => "down",
    }
}
