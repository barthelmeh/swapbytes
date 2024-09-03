use crate::network::Client;
use crate::ui::tabs;

use crate::APP;

use strum::{Display, EnumIter, FromRepr, IntoEnumIterator};

use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    prelude::*,
    widgets::*,
};
use std::error::Error;

#[derive(Default)]
pub struct ChatScreen {
    pub selected_tab: SelectedTab,
    pub chat: tabs::room::Room,
    pub select_room: tabs::select_room::SelectRoom,
}

impl ChatScreen {
    pub fn render(&mut self, frame: &mut Frame) {
        // As render always gets called, first check if there are no connected peers and we are on the select room tab.
        let app = APP.lock().unwrap();
        if app.num_connected_peers == 0 {
            self.selected_tab = SelectedTab::Chat;
        }
        drop(app);

        let vertical = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]);
        let [tab_area, content_area] = vertical.areas(frame.area());

        self.render_tabs(frame, tab_area);

        match self.selected_tab {
            SelectedTab::Chat => self.chat.render(frame, content_area),
            SelectedTab::SelectRoom => self.select_room.render(frame, content_area),
        }
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles = SelectedTab::iter().map(SelectedTab::title);
        let highlight_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let active_tab = self.selected_tab as usize;

        // Calculate the width used up by the tabs
        let total_title_width: usize = SelectedTab::iter().map(|t| t.title().width() + 2).sum();

        let tab_widget = Tabs::new(titles)
            .block(Block::new().style(Style::new()).padding(Padding::new(
                (area.width - total_title_width as u16) / 2,
                0,
                0,
                0,
            )))
            .highlight_style(highlight_style)
            .select(active_tab)
            .divider(" ");

        frame.render_widget(tab_widget, area);
    }

    pub(crate) async fn handle_events(
        &mut self,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error>> {
        // Handle events for the chat screen
        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        // Navigating tabs
                        KeyCode::Tab => {
                            // If no connected peers, do nothing
                            let app = APP.lock().unwrap();
                            if app.num_connected_peers == 0 {
                                return Ok(());
                            }

                            // If navigating to select_room, fetch rooms
                            match self.selected_tab {
                                SelectedTab::Chat => {
                                    app.fetch_rooms(client).await.unwrap();
                                }
                                _ => {}
                            }

                            drop(app);

                            self.selected_tab = self.selected_tab.next_tab();
                        }
                        // Closing the application
                        KeyCode::Esc => {
                            let mut app = APP.lock().unwrap();
                            app.quitting = true;
                            drop(app);
                        }
                        // Handle events on each tab
                        _ => match self.selected_tab {
                            SelectedTab::Chat => self.chat.handle_events(key, client).await?,
                            SelectedTab::SelectRoom => {
                                self.select_room
                                    .handle_events(key, client, &mut self.selected_tab)
                                    .await?
                            }
                        },
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Default, Clone, Copy, Display, FromRepr, EnumIter)]
pub enum SelectedTab {
    #[default]
    #[strum(to_string = "Chat")]
    Chat,
    #[strum(to_string = "Select Room")]
    SelectRoom,
}

impl SelectedTab {
    fn title(self) -> Line<'static> {
        format!(" {self} ")
            // .fg(tailwind::SLATE.c200)
            // .bg(self.palette().c900)
            .into()
    }

    fn next_tab(self) -> Self {
        let current_index = self as usize;
        let total_tabs = Self::iter().count();
        let next_index = (current_index + 1) % total_tabs;
        Self::from_repr(next_index).unwrap_or(self)
    }
}
