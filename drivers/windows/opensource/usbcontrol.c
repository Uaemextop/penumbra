/*
 * usbcontrol.c
 *
 * USB CDC ACM class-specific control requests:
 *   SET_LINE_CODING (0x20), GET_LINE_CODING (0x21),
 *   SET_CONTROL_LINE_STATE (0x22), SEND_BREAK (0x23)
 *
 * Equivalent to the original Ctrl_SetLineCoding, Ctrl_SetDtr, Ctrl_SetRts,
 * Ctrl_SendControlLineState functions (section 11).
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

#ifdef ALLOC_PRAGMA
#pragma alloc_text(PAGE, UsbControlSetLineCoding)
#pragma alloc_text(PAGE, UsbControlGetLineCoding)
#pragma alloc_text(PAGE, UsbControlSetControlLineState)
#pragma alloc_text(PAGE, UsbControlSendBreak)
#pragma alloc_text(PAGE, UsbControlSetDtr)
#pragma alloc_text(PAGE, UsbControlClrDtr)
#pragma alloc_text(PAGE, UsbControlSetRts)
#pragma alloc_text(PAGE, UsbControlClrRts)
#endif

/*
 * Helper: send a CDC ACM class-specific control request.
 * Uses WdfUsbTargetDeviceSendControlTransferSynchronously.
 *
 * For OUT requests (host -> device), the data is in TransferBuffer.
 * For no-data requests, TransferBuffer is NULL and TransferLength is 0.
 */
static
NTSTATUS
UsbControlSendCdcRequest(
    _In_        PDEVICE_CONTEXT DevCtx,
    _In_        UCHAR           Request,
    _In_        USHORT          Value,
    _In_opt_    PVOID           TransferBuffer,
    _In_        ULONG           TransferLength,
    _In_        BOOLEAN         DirectionIn
    )
{
    NTSTATUS                        status;
    WDF_USB_CONTROL_SETUP_PACKET    setupPacket;
    WDF_MEMORY_DESCRIPTOR           memDesc;
    ULONG                           bytesTransferred = 0;

    PAGED_CODE();

    /*
     * Build a class-specific, interface-directed setup packet.
     *
     * bmRequestType:
     *   Direction: Host-to-Device (0) or Device-to-Host (1)
     *   Type:      Class (01)
     *   Recipient: Interface (01)
     *
     * For SET_LINE_CODING:       OUT, class, interface, request=0x20
     * For GET_LINE_CODING:       IN,  class, interface, request=0x21
     * For SET_CONTROL_LINE_STATE: OUT, class, interface, request=0x22
     * For SEND_BREAK:            OUT, class, interface, request=0x23
     */
    WDF_USB_CONTROL_SETUP_PACKET_INIT_CLASS(
        &setupPacket,
        DirectionIn ? BmRequestDeviceToHost : BmRequestHostToDevice,
        BmRequestToInterface,
        Request,
        Value,
        (USHORT)DevCtx->InterfaceNumber
        );

    if (TransferBuffer != NULL && TransferLength > 0) {
        WDF_MEMORY_DESCRIPTOR_INIT_BUFFER(&memDesc,
                                           TransferBuffer,
                                           TransferLength);

        status = WdfUsbTargetDeviceSendControlTransferSynchronously(
            DevCtx->UsbDevice,
            WDF_NO_HANDLE,
            NULL,       /* no send options (blocking) */
            &setupPacket,
            &memDesc,
            &bytesTransferred
            );
    } else {
        status = WdfUsbTargetDeviceSendControlTransferSynchronously(
            DevCtx->UsbDevice,
            WDF_NO_HANDLE,
            NULL,
            &setupPacket,
            NULL,       /* no memory descriptor for zero-length transfers */
            &bytesTransferred
            );
    }

    return status;
}

/* =========================================================================
 *  UsbControlSetLineCoding — CDC SET_LINE_CODING (0x20)
 *  Sends baud rate, data bits, parity, and stop bits to the device.
 *  Equivalent to the original Ctrl_SetLineCoding (section 11).
 * ========================================================================= */
NTSTATUS
UsbControlSetLineCoding(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    PAGED_CODE();

    return UsbControlSendCdcRequest(
        DevCtx,
        CDC_SET_LINE_CODING,
        0,                              /* wValue = 0 */
        &DevCtx->LineCoding,
        sizeof(CDC_LINE_CODING),
        FALSE                           /* Host -> Device */
        );
}

/* =========================================================================
 *  UsbControlGetLineCoding — CDC GET_LINE_CODING (0x21)
 *  Reads current line coding from the device.
 * ========================================================================= */
NTSTATUS
UsbControlGetLineCoding(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    CDC_LINE_CODING lineCoding;
    NTSTATUS        status;

    PAGED_CODE();

    RtlZeroMemory(&lineCoding, sizeof(lineCoding));

    status = UsbControlSendCdcRequest(
        DevCtx,
        CDC_GET_LINE_CODING,
        0,
        &lineCoding,
        sizeof(CDC_LINE_CODING),
        TRUE                            /* Device -> Host */
        );

    if (NT_SUCCESS(status)) {
        DevCtx->LineCoding = lineCoding;
    }

    return status;
}

/* =========================================================================
 *  UsbControlSetControlLineState — CDC SET_CONTROL_LINE_STATE (0x22)
 *  Controls DTR (bit 0) and RTS (bit 1) signals.
 *  Equivalent to the original Ctrl_SendControlLineState (section 11).
 * ========================================================================= */
NTSTATUS
UsbControlSetControlLineState(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ USHORT          Value
    )
{
    PAGED_CODE();

    return UsbControlSendCdcRequest(
        DevCtx,
        CDC_SET_CONTROL_LINE_STATE,
        Value,                          /* wValue = DTR|RTS bitmap */
        NULL,
        0,
        FALSE
        );
}

/* =========================================================================
 *  UsbControlSendBreak — CDC SEND_BREAK (0x23)
 *  Duration in ms: 0 = off, 0xFFFF = on until explicitly turned off.
 * ========================================================================= */
NTSTATUS
UsbControlSendBreak(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ USHORT          Duration
    )
{
    PAGED_CODE();

    return UsbControlSendCdcRequest(
        DevCtx,
        CDC_SEND_BREAK,
        Duration,                       /* wValue = duration in ms */
        NULL,
        0,
        FALSE
        );
}

/* =========================================================================
 *  UsbControlSetDtr — assert DTR line
 *  Equivalent to the original Ctrl_SetDtr (section 11)
 * ========================================================================= */
NTSTATUS
UsbControlSetDtr(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    USHORT value;

    PAGED_CODE();

    DevCtx->DtrState = TRUE;

    value = CDC_CTL_DTR;
    if (DevCtx->RtsState) {
        value |= CDC_CTL_RTS;
    }

    return UsbControlSetControlLineState(DevCtx, value);
}

/* =========================================================================
 *  UsbControlClrDtr — deassert DTR line
 *  Equivalent to the original Ctrl_ClrDtr (section 11)
 * ========================================================================= */
NTSTATUS
UsbControlClrDtr(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    USHORT value = 0;

    PAGED_CODE();

    DevCtx->DtrState = FALSE;

    if (DevCtx->RtsState) {
        value |= CDC_CTL_RTS;
    }

    return UsbControlSetControlLineState(DevCtx, value);
}

/* =========================================================================
 *  UsbControlSetRts — assert RTS line
 *  Equivalent to the original Ctrl_SetRts (section 11)
 * ========================================================================= */
NTSTATUS
UsbControlSetRts(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    USHORT value;

    PAGED_CODE();

    DevCtx->RtsState = TRUE;

    value = CDC_CTL_RTS;
    if (DevCtx->DtrState) {
        value |= CDC_CTL_DTR;
    }

    return UsbControlSetControlLineState(DevCtx, value);
}

/* =========================================================================
 *  UsbControlClrRts — deassert RTS line
 *  Equivalent to the original Ctrl_ClrRts (section 11)
 * ========================================================================= */
NTSTATUS
UsbControlClrRts(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    USHORT value = 0;

    PAGED_CODE();

    DevCtx->RtsState = FALSE;

    if (DevCtx->DtrState) {
        value |= CDC_CTL_DTR;
    }

    return UsbControlSetControlLineState(DevCtx, value);
}
