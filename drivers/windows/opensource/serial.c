/*
 * serial.c
 *
 * Implementation of ALL IOCTL_SERIAL_* handlers.
 * Equivalent to the original Ctrl_* functions (section 8 / section 11).
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

/* =========================================================================
 *  IOCTL_SERIAL_SET_BAUD_RATE
 * ========================================================================= */
VOID
SerialSetBaudRate(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_BAUD_RATE   pBaud;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(SERIAL_BAUD_RATE), (PVOID *)&pBaud, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    if (pBaud->BaudRate == 0) {
        WdfRequestComplete(Request, STATUS_INVALID_PARAMETER);
        return;
    }

    DevCtx->LineCoding.dwDTERate = pBaud->BaudRate;

    /* Send updated line coding to device via CDC SET_LINE_CODING */
    status = UsbControlSetLineCoding(DevCtx);

    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_BAUD_RATE
 * ========================================================================= */
VOID
SerialGetBaudRate(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_BAUD_RATE   pBaud;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIAL_BAUD_RATE), (PVOID *)&pBaud, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    pBaud->BaudRate = DevCtx->LineCoding.dwDTERate;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIAL_BAUD_RATE));
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_LINE_CONTROL
 * ========================================================================= */
VOID
SerialSetLineControl(
    _In_ PDEVICE_CONTEXT    DevCtx,
    _In_ WDFREQUEST         Request
    )
{
    NTSTATUS                status;
    PSERIAL_LINE_CONTROL    pLine;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(SERIAL_LINE_CONTROL), (PVOID *)&pLine, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /* Map Win32 stop bits to CDC bCharFormat */
    switch (pLine->StopBits) {
    case STOP_BIT_1:    DevCtx->LineCoding.bCharFormat = 0; break;
    case STOP_BITS_1_5: DevCtx->LineCoding.bCharFormat = 1; break;
    case STOP_BITS_2:   DevCtx->LineCoding.bCharFormat = 2; break;
    default:
        WdfRequestComplete(Request, STATUS_INVALID_PARAMETER);
        return;
    }

    /* Map Win32 parity to CDC bParityType */
    switch (pLine->Parity) {
    case NO_PARITY:     DevCtx->LineCoding.bParityType = 0; break;
    case ODD_PARITY:    DevCtx->LineCoding.bParityType = 1; break;
    case EVEN_PARITY:   DevCtx->LineCoding.bParityType = 2; break;
    case MARK_PARITY:   DevCtx->LineCoding.bParityType = 3; break;
    case SPACE_PARITY:  DevCtx->LineCoding.bParityType = 4; break;
    default:
        WdfRequestComplete(Request, STATUS_INVALID_PARAMETER);
        return;
    }

    /* Data bits: 5, 6, 7, 8 */
    if (pLine->WordLength < 5 || pLine->WordLength > 8) {
        WdfRequestComplete(Request, STATUS_INVALID_PARAMETER);
        return;
    }
    DevCtx->LineCoding.bDataBits = pLine->WordLength;

    /* Apply via CDC SET_LINE_CODING */
    status = UsbControlSetLineCoding(DevCtx);

    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_LINE_CONTROL
 * ========================================================================= */
VOID
SerialGetLineControl(
    _In_ PDEVICE_CONTEXT    DevCtx,
    _In_ WDFREQUEST         Request
    )
{
    NTSTATUS                status;
    PSERIAL_LINE_CONTROL    pLine;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIAL_LINE_CONTROL), (PVOID *)&pLine, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /* Map CDC bCharFormat to Win32 stop bits */
    switch (DevCtx->LineCoding.bCharFormat) {
    case 0:  pLine->StopBits = STOP_BIT_1;    break;
    case 1:  pLine->StopBits = STOP_BITS_1_5; break;
    case 2:  pLine->StopBits = STOP_BITS_2;   break;
    default: pLine->StopBits = STOP_BIT_1;    break;
    }

    /* Map CDC bParityType to Win32 parity */
    switch (DevCtx->LineCoding.bParityType) {
    case 0:  pLine->Parity = NO_PARITY;    break;
    case 1:  pLine->Parity = ODD_PARITY;   break;
    case 2:  pLine->Parity = EVEN_PARITY;  break;
    case 3:  pLine->Parity = MARK_PARITY;  break;
    case 4:  pLine->Parity = SPACE_PARITY; break;
    default: pLine->Parity = NO_PARITY;    break;
    }

    pLine->WordLength = DevCtx->LineCoding.bDataBits;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIAL_LINE_CONTROL));
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_HANDFLOW
 * ========================================================================= */
VOID
SerialSetHandflow(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_HANDFLOW    pFlow;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(SERIAL_HANDFLOW), (PVOID *)&pFlow, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /*
     * Store handshake flow control settings.
     * CDC ACM doesn't directly support hardware flow control,
     * but we store the values for app compatibility.
     */
    DevCtx->HandFlow = *pFlow;

    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_HANDFLOW
 * ========================================================================= */
VOID
SerialGetHandflow(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_HANDFLOW    pFlow;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIAL_HANDFLOW), (PVOID *)&pFlow, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pFlow = DevCtx->HandFlow;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIAL_HANDFLOW));
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_DTR
 * ========================================================================= */
VOID
SerialSetDtr(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS status = UsbControlSetDtr(DevCtx);
    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_CLR_DTR
 * ========================================================================= */
VOID
SerialClrDtr(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS status = UsbControlClrDtr(DevCtx);
    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_RTS
 * ========================================================================= */
VOID
SerialSetRts(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS status = UsbControlSetRts(DevCtx);
    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_CLR_RTS
 * ========================================================================= */
VOID
SerialClrRts(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS status = UsbControlClrRts(DevCtx);
    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_DTRRTS
 * ========================================================================= */
VOID
SerialGetDtrRts(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PULONG      pDtrRts;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(ULONG), (PVOID *)&pDtrRts, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pDtrRts = 0;
    if (DevCtx->DtrState) {
        *pDtrRts |= SERIAL_DTR_STATE;
    }
    if (DevCtx->RtsState) {
        *pDtrRts |= SERIAL_RTS_STATE;
    }

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, sizeof(ULONG));
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_MODEMSTATUS
 * ========================================================================= */
VOID
SerialGetModemStatus(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PULONG      pStatus;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(ULONG), (PVOID *)&pStatus, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pStatus = DevCtx->ModemStatus;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, sizeof(ULONG));
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_BREAK_ON
 * ========================================================================= */
VOID
SerialSetBreakOn(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    /* CDC SEND_BREAK with 0xFFFF = turn on break indefinitely */
    NTSTATUS status = UsbControlSendBreak(DevCtx, 0xFFFF);
    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_BREAK_OFF
 * ========================================================================= */
VOID
SerialSetBreakOff(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    /* CDC SEND_BREAK with 0 = turn off break */
    NTSTATUS status = UsbControlSendBreak(DevCtx, 0);
    WdfRequestComplete(Request, status);
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_TIMEOUTS
 * ========================================================================= */
VOID
SerialSetTimeouts(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_TIMEOUTS    pTimeouts;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(SERIAL_TIMEOUTS), (PVOID *)&pTimeouts, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /*
     * Validate: if both ReadIntervalTimeout and ReadTotalTimeoutConstant
     * are MAXULONG but ReadTotalTimeoutMultiplier is not MAXULONG and not 0,
     * it's invalid.
     */
    if (pTimeouts->ReadIntervalTimeout == MAXULONG &&
        pTimeouts->ReadTotalTimeoutConstant == MAXULONG &&
        pTimeouts->ReadTotalTimeoutMultiplier != 0 &&
        pTimeouts->ReadTotalTimeoutMultiplier != MAXULONG) {
        WdfRequestComplete(Request, STATUS_INVALID_PARAMETER);
        return;
    }

    DevCtx->Timeouts = *pTimeouts;

    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_TIMEOUTS
 * ========================================================================= */
VOID
SerialGetTimeouts(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_TIMEOUTS    pTimeouts;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIAL_TIMEOUTS), (PVOID *)&pTimeouts, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pTimeouts = DevCtx->Timeouts;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIAL_TIMEOUTS));
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_CHARS
 * ========================================================================= */
VOID
SerialSetChars(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS        status;
    PSERIAL_CHARS   pChars;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(SERIAL_CHARS), (PVOID *)&pChars, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    DevCtx->SpecialChars = *pChars;

    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_CHARS
 * ========================================================================= */
VOID
SerialGetChars(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS        status;
    PSERIAL_CHARS   pChars;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIAL_CHARS), (PVOID *)&pChars, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pChars = DevCtx->SpecialChars;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIAL_CHARS));
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_QUEUE_SIZE
 * ========================================================================= */
VOID
SerialSetQueueSize(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_QUEUE_SIZE  pSize;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(SERIAL_QUEUE_SIZE), (PVOID *)&pSize, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /*
     * Store requested queue sizes. The actual ring buffer size is fixed,
     * but we report these values back for compatibility.
     */
    if (pSize->InSize > 0) {
        DevCtx->InQueueSize = pSize->InSize;
    }
    if (pSize->OutSize > 0) {
        DevCtx->OutQueueSize = pSize->OutSize;
    }

    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  IOCTL_SERIAL_SET_WAIT_MASK
 * ========================================================================= */
VOID
SerialSetWaitMask(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PULONG      pMask;
    WDFREQUEST  oldWaitRequest;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(ULONG), (PVOID *)&pMask, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /*
     * Setting a new wait mask cancels any pending WAIT_ON_MASK.
     * Complete the old one with events = 0.
     */
    status = WdfIoQueueRetrieveNextRequest(
        DevCtx->PendingWaitMaskQueue, &oldWaitRequest);
    if (NT_SUCCESS(status)) {
        PULONG pEvents;
        NTSTATUS st2 = WdfRequestRetrieveOutputBuffer(
            oldWaitRequest, sizeof(ULONG), (PVOID *)&pEvents, NULL);
        if (NT_SUCCESS(st2)) {
            *pEvents = 0;
        }
        WdfRequestCompleteWithInformation(oldWaitRequest, STATUS_SUCCESS,
                                          sizeof(ULONG));
    }

    WdfSpinLockAcquire(DevCtx->EventLock);
    DevCtx->WaitMask    = *pMask;
    DevCtx->EventHistory = 0;
    WdfSpinLockRelease(DevCtx->EventLock);

    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_WAIT_MASK
 * ========================================================================= */
VOID
SerialGetWaitMask(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PULONG      pMask;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(ULONG), (PVOID *)&pMask, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pMask = DevCtx->WaitMask;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, sizeof(ULONG));
}

/* =========================================================================
 *  IOCTL_SERIAL_WAIT_ON_MASK
 *  Equivalent to the original Ctrl_WaitOnMask (section 8)
 * ========================================================================= */
VOID
SerialWaitOnMask(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PULONG      pEvents;
    ULONG       currentEvents;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(ULONG), (PVOID *)&pEvents, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    if (DevCtx->WaitMask == 0) {
        *pEvents = 0;
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                          sizeof(ULONG));
        return;
    }

    /* Check if events have already occurred */
    WdfSpinLockAcquire(DevCtx->EventLock);
    currentEvents = DevCtx->EventHistory & DevCtx->WaitMask;
    DevCtx->EventHistory = 0;
    WdfSpinLockRelease(DevCtx->EventLock);

    if (currentEvents != 0) {
        *pEvents = currentEvents;
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                          sizeof(ULONG));
        return;
    }

    /* No events yet — queue the request for later completion */
    status = WdfRequestForwardToIoQueue(Request, DevCtx->PendingWaitMaskQueue);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
    }
}

/* =========================================================================
 *  SerialCompleteWaitOnMask — complete a pending WAIT_ON_MASK request
 *  Called from interrupt notification and other event sources.
 * ========================================================================= */
VOID
SerialCompleteWaitOnMask(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ ULONG           Events
    )
{
    NTSTATUS    status;
    WDFREQUEST  waitRequest;
    ULONG       matchingEvents;

    WdfSpinLockAcquire(DevCtx->EventLock);
    DevCtx->EventHistory |= Events;
    matchingEvents = DevCtx->EventHistory & DevCtx->WaitMask;
    if (matchingEvents != 0) {
        DevCtx->EventHistory = 0;
    }
    WdfSpinLockRelease(DevCtx->EventLock);

    if (matchingEvents == 0) {
        return;
    }

    /* Try to dequeue and complete the pending wait-on-mask request */
    status = WdfIoQueueRetrieveNextRequest(
        DevCtx->PendingWaitMaskQueue, &waitRequest);
    if (NT_SUCCESS(status)) {
        PULONG pEvents;
        NTSTATUS st2 = WdfRequestRetrieveOutputBuffer(
            waitRequest, sizeof(ULONG), (PVOID *)&pEvents, NULL);
        if (NT_SUCCESS(st2)) {
            *pEvents = matchingEvents;
        }
        WdfRequestCompleteWithInformation(waitRequest, STATUS_SUCCESS,
                                          sizeof(ULONG));
    }
}

/* =========================================================================
 *  IOCTL_SERIAL_PURGE
 *  Equivalent to the original Ctrl_Purge
 * ========================================================================= */
VOID
SerialPurge(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PULONG      pPurgeFlags;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(ULONG), (PVOID *)&pPurgeFlags, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    if (*pPurgeFlags & SERIAL_PURGE_RXCLEAR) {
        /* Clear the ring buffer */
        RingBufferPurge(&DevCtx->ReadBuffer);
    }

    if (*pPurgeFlags & SERIAL_PURGE_RXABORT) {
        /* Cancel all pending read requests */
        WdfIoQueuePurgeSynchronously(DevCtx->PendingReadQueue);
        WdfIoQueueStart(DevCtx->PendingReadQueue);
    }

    if (*pPurgeFlags & SERIAL_PURGE_TXCLEAR) {
        /* Nothing to clear for TX (data already submitted to USB) */
    }

    if (*pPurgeFlags & SERIAL_PURGE_TXABORT) {
        /*
         * For TX abort, we would need to cancel outstanding USB writes.
         * The USB stack will cancel them on pipe reset if needed.
         */
    }

    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_COMMSTATUS
 *  Equivalent to the original Ctrl_GetCommStatus
 * ========================================================================= */
VOID
SerialGetCommStatus(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS        status;
    PSERIAL_STATUS  pStatus;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIAL_STATUS), (PVOID *)&pStatus, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    RtlZeroMemory(pStatus, sizeof(SERIAL_STATUS));
    pStatus->AmountInInQueue  = RingBufferGetDataLength(&DevCtx->ReadBuffer);
    pStatus->AmountInOutQueue = 0;
    pStatus->EofReceived      = FALSE;
    pStatus->WaitForImmediate = FALSE;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIAL_STATUS));
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_PROPERTIES
 *  Equivalent to the original Ctrl_GetProperties
 * ========================================================================= */
VOID
SerialGetProperties(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIAL_COMMPROP    pProp;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIAL_COMMPROP), (PVOID *)&pProp, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    RtlZeroMemory(pProp, sizeof(SERIAL_COMMPROP));

    pProp->PacketLength       = sizeof(SERIAL_COMMPROP);
    pProp->PacketVersion      = 2;
    pProp->ServiceMask        = SERIAL_SP_SERIALCOMM;
    pProp->MaxTxQueue         = 0;
    pProp->MaxRxQueue         = 0;

    /* Supported baud rates */
    pProp->MaxBaud            = SERIAL_BAUD_USER;
    pProp->SettableBaud       = SERIAL_BAUD_300  | SERIAL_BAUD_600  |
                                SERIAL_BAUD_1200 | SERIAL_BAUD_2400 |
                                SERIAL_BAUD_4800 | SERIAL_BAUD_9600 |
                                SERIAL_BAUD_14400 | SERIAL_BAUD_19200 |
                                SERIAL_BAUD_38400 | SERIAL_BAUD_56K |
                                SERIAL_BAUD_57600 | SERIAL_BAUD_115200 |
                                SERIAL_BAUD_USER;

    pProp->ProvSubType        = SERIAL_SP_RS232;

    pProp->ProvCapabilities   = SERIAL_PCF_DTRDSR |
                                SERIAL_PCF_RTSCTS |
                                SERIAL_PCF_CD     |
                                SERIAL_PCF_PARITY_CHECK |
                                SERIAL_PCF_TOTALTIMEOUTS |
                                SERIAL_PCF_INTTIMEOUTS;

    pProp->SettableParams     = SERIAL_SP_PARITY |
                                SERIAL_SP_BAUD   |
                                SERIAL_SP_DATABITS |
                                SERIAL_SP_STOPBITS |
                                SERIAL_SP_HANDSHAKING |
                                SERIAL_SP_PARITY_CHECK |
                                SERIAL_SP_CARRIER_DETECT;

    pProp->SettableData       = SERIAL_DATABITS_5 |
                                SERIAL_DATABITS_6 |
                                SERIAL_DATABITS_7 |
                                SERIAL_DATABITS_8;

    pProp->SettableStopParity = SERIAL_STOPBITS_10 |
                                SERIAL_STOPBITS_15 |
                                SERIAL_STOPBITS_20 |
                                SERIAL_PARITY_NONE |
                                SERIAL_PARITY_ODD  |
                                SERIAL_PARITY_EVEN |
                                SERIAL_PARITY_MARK |
                                SERIAL_PARITY_SPACE;

    pProp->CurrentTxQueue     = DevCtx->OutQueueSize;
    pProp->CurrentRxQueue     = DevCtx->InQueueSize;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIAL_COMMPROP));
}

/* =========================================================================
 *  IOCTL_SERIAL_GET_STATS
 * ========================================================================= */
VOID
SerialGetStats(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS            status;
    PSERIALPERF_STATS   pStats;

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(SERIALPERF_STATS), (PVOID *)&pStats, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pStats = DevCtx->PerfStats;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS,
                                      sizeof(SERIALPERF_STATS));
}

/* =========================================================================
 *  IOCTL_SERIAL_CLEAR_STATS
 * ========================================================================= */
VOID
SerialClearStats(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    RtlZeroMemory(&DevCtx->PerfStats, sizeof(SERIALPERF_STATS));
    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  IOCTL_SERIAL_CONFIG_SIZE
 *  Returns 0 = no provider-specific data
 * ========================================================================= */
VOID
SerialConfigSize(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PULONG      pSize;

    UNREFERENCED_PARAMETER(DevCtx);

    status = WdfRequestRetrieveOutputBuffer(
        Request, sizeof(ULONG), (PVOID *)&pSize, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    *pSize = 0;

    WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, sizeof(ULONG));
}

/* =========================================================================
 *  IOCTL_SERIAL_LSRMST_INSERT
 *  Sets the escape character for LSR/MSR insertion
 * ========================================================================= */
VOID
SerialLsrMstInsert(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ WDFREQUEST      Request
    )
{
    NTSTATUS    status;
    PUCHAR      pEscape;

    status = WdfRequestRetrieveInputBuffer(
        Request, sizeof(UCHAR), (PVOID *)&pEscape, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    DevCtx->LsrMstInsert = *pEscape;
    DevCtx->LsrMstInsertEnabled = (*pEscape != 0);

    WdfRequestComplete(Request, STATUS_SUCCESS);
}
