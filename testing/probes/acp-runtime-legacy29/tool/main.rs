use acp_remote::adapters::acp::*;
fn inspect(client: &Connection, request: &PermissionRequest, other: &PermissionRequest) {
    let _ = (request.id(), request.session(), request.tool(), request.supported_subject(), request.options(), request.raw(), request.scope());
    let _ = request.options().first().map(|choice| (choice.id(), choice.kind(), choice.raw()));
    let _ = client.reserve_permission_response(&request.clone(), PermissionSelection::Cancelled);
    let _ = other.raw();
    let mut local = request.clone(); local.tool = None;
}
fn main() {}
