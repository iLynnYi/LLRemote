fn item(kind: TrayMenuItemKind, label: &str) -> TrayMenuItemModel {
    TrayMenuItemModel {
        kind,
        label: label.to_string(),
        payload: None,
        children: Vec::new(),
        enabled: true,
    }
}

pub(super) fn menu_item_id(item: &TrayMenuItemModel) -> String {
    match (&item.kind, &item.payload) {
        (TrayMenuItemKind::DisconnectLocalConnection, Some(payload)) => {
            format!("{}:{payload}", item.kind.id())
        }
        _ => item.kind.id().to_string(),
    }
}
