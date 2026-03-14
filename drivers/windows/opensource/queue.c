/*
 * queue.c
 *
 * I/O Queue initialisation and dispatch handlers for Read, Write,
 * and DeviceControl requests.
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

#ifdef ALLOC_PRAGMA
#pragma alloc_text(PAGE, QueueInitialize)
#endif

/* =========================================================================
 *  QueueInitialize — create all I/O queues
 * ========================================================================= */
NTSTATUS
QueueInitialize(
    _In_ WDFDEVICE Device
    )
{
    NTSTATUS                status;
    WDF_IO_QUEUE_CONFIG     queueConfig;
    PDEVICE_CONTEXT         devCtx = GetDeviceContext(Device);

    PAGED_CODE();

    /*
     * Default queue: parallel dispatch for Read, Write, and DeviceControl.
     * This allows multiple concurrent I/O operations.
     */
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig,
                                            WdfIoQueueDispatchParallel);
    queueConfig.EvtIoRead          = EvtIoRead;
    queueConfig.EvtIoWrite         = EvtIoWrite;
    queueConfig.EvtIoDeviceControl = EvtIoDeviceControl;
    queueConfig.PowerManaged       = WdfTrue;

    status = WdfIoQueueCreate(Device, &queueConfig,
                              WDF_NO_OBJECT_ATTRIBUTES,
                              &devCtx->DefaultQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /*
     * Manual queue for pending read requests.
     * Reads that cannot be satisfied immediately from the ring buffer
     * are forwarded here and completed when data arrives from USB.
     */
    WDF_IO_QUEUE_CONFIG_INIT(&queueConfig, WdfIoQueueDispatchManual);
    queueConfig.PowerManaged = WdfTrue;

    status = WdfIoQueueCreate(Device, &queueConfig,
                              WDF_NO_OBJECT_ATTRIBUTES,
                              &devCtx->PendingReadQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /*
     * Manual queue for pending IOCTL_SERIAL_WAIT_ON_MASK.
     * At most one request can be pending at a time.
     */
    WDF_IO_QUEUE_CONFIG_INIT(&queueConfig, WdfIoQueueDispatchManual);
    queueConfig.PowerManaged = WdfTrue;

    status = WdfIoQueueCreate(Device, &queueConfig,
                              WDF_NO_OBJECT_ATTRIBUTES,
                              &devCtx->PendingWaitMaskQueue);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  EvtIoRead — handle read requests
 *  Equivalent to the original DispatchRead (section 7)
 *
 *  Strategy:
 *    1. Try to satisfy the request from the ring buffer immediately
 *    2. If MAXULONG interval timeout, return whatever is available (or 0)
 *    3. If no data, forward to the pending-read queue for later completion
 * ========================================================================= */
VOID
EvtIoRead(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     Length
    )
{
    NTSTATUS        status;
    WDFDEVICE       device = WdfIoQueueGetDevice(Queue);
    PDEVICE_CONTEXT devCtx = GetDeviceContext(device);
    PVOID           buffer;
    ULONG           bytesRead;

    UNREFERENCED_PARAMETER(Length);

    if (Length == 0) {
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, 0);
        return;
    }

    /* Get the output buffer from the request */
    status = WdfRequestRetrieveOutputBuffer(Request, 1, &buffer, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /* Try to fill from ring buffer */
    bytesRead = RingBufferRead(
        &devCtx->ReadBuffer,
        (PUCHAR)buffer,
        (ULONG)Length
        );

    if (bytesRead > 0) {
        /* Data available — complete immediately */
        devCtx->PerfStats.ReceivedCount += bytesRead;
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, bytesRead);
        return;
    }

    /*
     * Check for MAXULONG interval timeout — this means
     * "return immediately with whatever is available".
     * Since bytesRead is 0 here and the timeout says return immediately,
     * complete with 0 bytes.
     */
    if (devCtx->Timeouts.ReadIntervalTimeout == MAXULONG &&
        devCtx->Timeouts.ReadTotalTimeoutMultiplier == 0 &&
        devCtx->Timeouts.ReadTotalTimeoutConstant == 0) {
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, 0);
        return;
    }

    /*
     * Check for MAXULONG special case: ReadIntervalTimeout=MAXULONG,
     * ReadTotalTimeoutMultiplier=MAXULONG, ReadTotalTimeoutConstant>0
     * means "wait at most ReadTotalTimeoutConstant ms for any data".
     * For now, queue and let the continuous reader complete it.
     */

    /* No data — queue the request for later completion */
    status = WdfRequestForwardToIoQueue(Request, devCtx->PendingReadQueue);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
    }
}

/* =========================================================================
 *  EvtIoWrite — handle write requests
 *  Equivalent to the original DispatchWrite (section 7)
 *
 *  Strategy: format the request for a USB bulk OUT transfer and
 *  send it asynchronously with a completion routine.
 * ========================================================================= */
VOID
EvtIoWrite(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     Length
    )
{
    NTSTATUS            status;
    WDFDEVICE           device = WdfIoQueueGetDevice(Queue);
    PDEVICE_CONTEXT     devCtx = GetDeviceContext(device);
    WDFMEMORY           reqMemory;
    WDF_REQUEST_SEND_OPTIONS sendOpts;

    if (Length == 0) {
        WdfRequestCompleteWithInformation(Request, STATUS_SUCCESS, 0);
        return;
    }

    if (devCtx->BulkOutPipe == NULL) {
        WdfRequestComplete(Request, STATUS_INVALID_DEVICE_STATE);
        return;
    }

    /* Retrieve the input memory from the write request */
    status = WdfRequestRetrieveInputMemory(Request, &reqMemory);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /* Format the request for bulk OUT pipe */
    status = WdfUsbTargetPipeFormatRequestForWrite(
        devCtx->BulkOutPipe,
        Request,
        reqMemory,
        NULL    /* no offset */
        );
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    /* Set completion routine for ZLP check and stats */
    WdfRequestSetCompletionRoutine(
        Request,
        EvtWriteRequestComplete,
        devCtx
        );

    /* Configure send timeout if WriteTotalTimeout is set */
    WDF_REQUEST_SEND_OPTIONS_INIT(&sendOpts, 0);
    if (devCtx->Timeouts.WriteTotalTimeoutMultiplier != 0 ||
        devCtx->Timeouts.WriteTotalTimeoutConstant != 0) {
        LONGLONG timeoutMs =
            (LONGLONG)devCtx->Timeouts.WriteTotalTimeoutMultiplier * (LONGLONG)Length +
            (LONGLONG)devCtx->Timeouts.WriteTotalTimeoutConstant;
        if (timeoutMs > 0) {
            WDF_REQUEST_SEND_OPTIONS_SET_TIMEOUT(
                &sendOpts,
                WDF_REL_TIMEOUT_IN_MS(timeoutMs)
                );
        }
    }

    InterlockedIncrement(&devCtx->OutstandingWrites);

    if (!WdfRequestSend(
            Request,
            WdfUsbTargetPipeGetIoTarget(devCtx->BulkOutPipe),
            &sendOpts)) {
        InterlockedDecrement(&devCtx->OutstandingWrites);
        status = WdfRequestGetStatus(Request);
        WdfRequestComplete(Request, status);
    }
}

/* =========================================================================
 *  EvtIoDeviceControl — handle serial port IOCTLs
 *  Equivalent to the original DispatchIoCtrl (section 8)
 * ========================================================================= */
VOID
EvtIoDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
    )
{
    WDFDEVICE       device = WdfIoQueueGetDevice(Queue);
    PDEVICE_CONTEXT devCtx = GetDeviceContext(device);

    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    switch (IoControlCode) {

    /* ---- Baud Rate ---- */
    case IOCTL_SERIAL_SET_BAUD_RATE:
        SerialSetBaudRate(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_BAUD_RATE:
        SerialGetBaudRate(devCtx, Request);
        return;

    /* ---- Line Control ---- */
    case IOCTL_SERIAL_SET_LINE_CONTROL:
        SerialSetLineControl(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_LINE_CONTROL:
        SerialGetLineControl(devCtx, Request);
        return;

    /* ---- Handshake / Flow Control ---- */
    case IOCTL_SERIAL_SET_HANDFLOW:
        SerialSetHandflow(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_HANDFLOW:
        SerialGetHandflow(devCtx, Request);
        return;

    /* ---- DTR/RTS ---- */
    case IOCTL_SERIAL_SET_DTR:
        SerialSetDtr(devCtx, Request);
        return;
    case IOCTL_SERIAL_CLR_DTR:
        SerialClrDtr(devCtx, Request);
        return;
    case IOCTL_SERIAL_SET_RTS:
        SerialSetRts(devCtx, Request);
        return;
    case IOCTL_SERIAL_CLR_RTS:
        SerialClrRts(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_DTRRTS:
        SerialGetDtrRts(devCtx, Request);
        return;

    /* ---- Modem Status ---- */
    case IOCTL_SERIAL_GET_MODEMSTATUS:
        SerialGetModemStatus(devCtx, Request);
        return;

    /* ---- Break ---- */
    case IOCTL_SERIAL_SET_BREAK_ON:
        SerialSetBreakOn(devCtx, Request);
        return;
    case IOCTL_SERIAL_SET_BREAK_OFF:
        SerialSetBreakOff(devCtx, Request);
        return;

    /* ---- Timeouts ---- */
    case IOCTL_SERIAL_SET_TIMEOUTS:
        SerialSetTimeouts(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_TIMEOUTS:
        SerialGetTimeouts(devCtx, Request);
        return;

    /* ---- Special Characters ---- */
    case IOCTL_SERIAL_SET_CHARS:
        SerialSetChars(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_CHARS:
        SerialGetChars(devCtx, Request);
        return;

    /* ---- Queue Size ---- */
    case IOCTL_SERIAL_SET_QUEUE_SIZE:
        SerialSetQueueSize(devCtx, Request);
        return;

    /* ---- Wait Mask ---- */
    case IOCTL_SERIAL_SET_WAIT_MASK:
        SerialSetWaitMask(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_WAIT_MASK:
        SerialGetWaitMask(devCtx, Request);
        return;
    case IOCTL_SERIAL_WAIT_ON_MASK:
        SerialWaitOnMask(devCtx, Request);
        return;

    /* ---- Purge ---- */
    case IOCTL_SERIAL_PURGE:
        SerialPurge(devCtx, Request);
        return;

    /* ---- Status & Properties ---- */
    case IOCTL_SERIAL_GET_COMMSTATUS:
        SerialGetCommStatus(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_PROPERTIES:
        SerialGetProperties(devCtx, Request);
        return;
    case IOCTL_SERIAL_GET_STATS:
        SerialGetStats(devCtx, Request);
        return;
    case IOCTL_SERIAL_CLEAR_STATS:
        SerialClearStats(devCtx, Request);
        return;
    case IOCTL_SERIAL_CONFIG_SIZE:
        SerialConfigSize(devCtx, Request);
        return;

    /* ---- LSR/MSR Insertion ---- */
    case IOCTL_SERIAL_LSRMST_INSERT:
        SerialLsrMstInsert(devCtx, Request);
        return;

    default:
        WdfRequestComplete(Request, STATUS_INVALID_DEVICE_REQUEST);
        return;
    }
}
