use iced::widget::{button, checkbox, row, text, Space};
use iced::{
    alignment, color, Alignment, Background, BorderRadius, Command, Element, Length, Renderer,
    Theme,
};

#[derive(Clone, Debug)]
pub struct ModRow {
    pub name: String,
    pub url: String,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    ModPressed,
    ToggleSelection(bool),
}

impl ModRow {
    pub fn new(name: String, url: String, selected: bool) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
            selected,
        }
    }

    pub fn update(&mut self, _message: &Message) -> Command<Message> {
        Command::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        // let button_style;
        // let action_text;
        let selection_checkbox;

        let mut checkbox_style = checkbox::Appearance {
            background: Background::Color(color!(0, 0, 0)),
            border_color: color!(0, 0, 0),
            border_radius: 5.0.into(),
            border_width: 1.0,
            icon_color: color!(0, 0, 0),
            text_color: None,
        };

        if self.selected.clone() {
            // todo change colour of row?
            // checkbox_style = checkbox::Appearance {
            //     background: Background::Color(color!(0,0,0)),
            //     border_color: color!(0,0,0),
            //     border_radius: 5.0.into(),
            //     border_width: 1.0,
            //     icon_color: color!(0,0,0),
            //     text_color: None
            // };
        }

        selection_checkbox = checkbox("", self.selected, Message::ToggleSelection);
        // selection_checkbox = checkbox("", self.selected, Message::ToggleSelection).style(checkbox_style);

        row![
            button(
                row![
                    text(&self.name).width(Length::FillPortion(8)),
                    text(&self.url).width(Length::FillPortion(8)),
                    selection_checkbox,
                ]
                .align_items(Alignment::Center)
            )
            .padding(8)
            // .style(if self.current {
            //     style::Button::SelectedPackage
            // } else {
            //     style::Button::NormalPackage
            // })
            .width(Length::Fill)
            .on_press(Message::ToggleSelection(!self.selected)),
            Space::with_width(15)
        ]
        .align_items(Alignment::Center)
        .into()
    }
}
