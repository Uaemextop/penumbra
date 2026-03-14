/*
 * mtk_usb2ser.h
 *
 * Main header for the MediaTek USB CDC ACM to Serial Port KMDF driver.
 * Declares device context, ring buffer, constants, GUIDs, and all
 * function prototypes used across the driver modules.
 *
 * SPDX-License-Identifier: MIT
 */

#pragma once

#include <ntddk.h>
#include <wdf.h>
#include <wdfusb.h>
#include <ntstrsafe.h>
#include <initguid.h>
#include <wmilib.h>
#include <wmidata.h>
#include <ntddser.h>

#include "version.h"

/* =========================================================================
 *  GUIDs
 * ========================================================================= */

/*
 * Device interface GUID for the MTK CDC ACM COM port.
 * Applications and WMI use this to discover our device.
 */
DEFINE_GUID(GUID_MTK_DEVINTERFACE_COMPORT,
    0x86e0d1e0, 0x8089, 0x11d0,
    0x9c, 0xe4, 0x08, 0x00, 0x3e, 0x30, 0x1f, 0x73);

/* =========================================================================
 *  Constants
 * ========================================================================= */

/* Default serial port parameters */
#define DEFAULT_BAUD_RATE           115200
#define DEFAULT_DATA_BITS           8

/* Queue / buffer sizes */
#define DEFAULT_IN_QUEUE_SIZE       4096
#define DEFAULT_OUT_QUEUE_SIZE      4096
#define DEFAULT_READ_BUFFER_SIZE    (64 * 1024)   /* 64 KB ring buffer */

/* USB transfer sizes */
#define MAX_TRANSFER_SIZE           4096
#define INTERRUPT_BUFFER_SIZE       16
#define NUM_CONTINUOUS_READERS      2

/* Power / Idle defaults */
#define DEFAULT_IDLE_TIME_SEC       10

/* =========================================================================
 *  CDC ACM Class-Specific Definitions
 * ========================================================================= */

/* CDC ACM class request codes */
#define CDC_SET_LINE_CODING         0x20
#define CDC_GET_LINE_CODING         0x21
#define CDC_SET_CONTROL_LINE_STATE  0x22
#define CDC_SEND_BREAK              0x23

/* Control line state bitmap bits (wValue for SET_CONTROL_LINE_STATE) */
#define CDC_CTL_DTR                 0x0001
#define CDC_CTL_RTS                 0x0002

/* CDC serial state notification bits (from interrupt endpoint) */
#define CDC_SERIAL_STATE_DCD        0x0001  /* bRxCarrier */
#define CDC_SERIAL_STATE_DSR        0x0002  /* bTxCarrier */
#define CDC_SERIAL_STATE_BREAK      0x0004
#define CDC_SERIAL_STATE_RING       0x0008
#define CDC_SERIAL_STATE_FRAMING    0x0010
#define CDC_SERIAL_STATE_PARITY     0x0020
#define CDC_SERIAL_STATE_OVERRUN    0x0040

/*
 * CDC Line Coding structure (7 bytes, packed).
 * Sent/received via SET_LINE_CODING / GET_LINE_CODING control requests.
 */
#include <pshpack1.h>
typedef struct _CDC_LINE_CODING {
    ULONG   dwDTERate;      /* Baud rate (bits per second) */
    UCHAR   bCharFormat;    /* Stop bits: 0=1, 1=1.5, 2=2 */
    UCHAR   bParityType;    /* Parity: 0=None, 1=Odd, 2=Even, 3=Mark, 4=Space */
    UCHAR   bDataBits;      /* Data bits: 5, 6, 7, 8, or 16 */
} CDC_LINE_CODING, *PCDC_LINE_CODING;
#include <poppack.h>

/*
 * CDC Notification Header (for interrupt endpoint data).
 */
#include <pshpack1.h>
typedef struct _CDC_NOTIFICATION {
    UCHAR   bmRequestType;
    UCHAR   bNotification;
    USHORT  wValue;
    USHORT  wIndex;
    USHORT  wLength;
} CDC_NOTIFICATION, *PCDC_NOTIFICATION;
#include <poppack.h>

#define CDC_NOTIFICATION_SERIAL_STATE   0x20

/* =========================================================================
 *  Ring Buffer
 * ========================================================================= */

typedef struct _RING_BUFFER {
    PUCHAR      Buffer;         /* Allocated circular buffer */
    ULONG       Size;           /* Total buffer capacity */
    ULONG       Head;           /* Write position */
    ULONG       Tail;           /* Read position */
    ULONG       DataLength;     /* Current amount of data in the buffer */
    WDFSPINLOCK Lock;           /* Spin lock protecting this ring buffer */
} RING_BUFFER, *PRING_BUFFER;

/* =========================================================================
 *  Device Context
 * ========================================================================= */

typedef struct _DEVICE_CONTEXT {
    /* Framework handles */
    WDFDEVICE           Device;
    WDFUSBDEVICE        UsbDevice;
    WDFUSBINTERFACE     UsbInterface;

    /* USB pipe handles */
    WDFUSBPIPE          BulkInPipe;
    WDFUSBPIPE          BulkOutPipe;
    WDFUSBPIPE          InterruptPipe;
    USHORT              BulkInMaxPacket;
    USHORT              BulkOutMaxPacket;
    UCHAR               InterfaceNumber;

    /* I/O queues */
    WDFQUEUE            DefaultQueue;
    WDFQUEUE            PendingReadQueue;
    WDFQUEUE            PendingWaitMaskQueue;

    /* ZLP (zero-length packet) work item for aligned writes */
    WDFWORKITEM         ZlpWorkItem;

    /* Ring buffer for incoming serial data */
    RING_BUFFER         ReadBuffer;

    /* Serial port state */
    CDC_LINE_CODING     LineCoding;
    SERIAL_TIMEOUTS     Timeouts;
    SERIAL_HANDFLOW     Handflow;
    SERIAL_CHARS        SpecialChars;
    SERIALPERF_STATS    PerfStats;

    BOOLEAN             DtrState;
    BOOLEAN             RtsState;
    ULONG               ModemStatus;

    /* Event / wait mask */
    WDFSPINLOCK         EventLock;
    ULONG               WaitMask;
    ULONG               EventHistory;

    /* Write tracking */
    WDFSPINLOCK         WriteLock;
    LONG                OutstandingWrites;

    /* Queue sizes (for SERIAL_COMMPROP) */
    ULONG               InQueueSize;
    ULONG               OutQueueSize;

    /* LSR/MSR insertion */
    UCHAR               LsrMstInsert;
    BOOLEAN             LsrMstInsertEnabled;

    /* Device naming */
    LONG                PortIndex;
    UNICODE_STRING      DeviceName;
    WCHAR               DeviceNameBuf[64];
    UNICODE_STRING      PortName;
    WCHAR               PortNameBuf[32];
    UNICODE_STRING      DosName;
    WCHAR               DosNameBuf[64];
    BOOLEAN             SymbolicLinkCreated;
    BOOLEAN             SerialCommWritten;

    /* Device state */
    BOOLEAN             DeviceStarted;
    BOOLEAN             DeviceOpen;

    /* Power / idle settings (from registry) */
    ULONG               IdleTimeSeconds;
    BOOLEAN             IdleEnabled;
    BOOLEAN             IdleWWBound;

} DEVICE_CONTEXT, *PDEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DEVICE_CONTEXT, GetDeviceContext)

/* =========================================================================
 *  Function Prototypes — driver.c
 * ========================================================================= */

DRIVER_INITIALIZE       DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD  EvtDriverDeviceAdd;
EVT_WDF_OBJECT_CONTEXT_CLEANUP EvtDriverCleanup;

/* =========================================================================
 *  Function Prototypes — device.c
 * ========================================================================= */

EVT_WDF_DEVICE_PREPARE_HARDWARE  EvtDevicePrepareHardware;
EVT_WDF_DEVICE_RELEASE_HARDWARE  EvtDeviceReleaseHardware;
EVT_WDF_DEVICE_D0_ENTRY          EvtDeviceD0Entry;
EVT_WDF_DEVICE_D0_EXIT           EvtDeviceD0Exit;
EVT_WDF_DEVICE_FILE_CREATE       EvtDeviceFileCreate;
EVT_WDF_FILE_CLOSE               EvtFileClose;
EVT_WDF_FILE_CLEANUP             EvtFileCleanup;

NTSTATUS
DeviceConfigureUsbDevice(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
DeviceConfigureUsbPipes(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
DeviceCreateSymbolicLink(
    _In_ PDEVICE_CONTEXT DevCtx
    );

VOID
DeviceRemoveSymbolicLink(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
DeviceReadRegistrySettings(
    _In_ PDEVICE_CONTEXT DevCtx
    );

/* =========================================================================
 *  Function Prototypes — queue.c
 * ========================================================================= */

NTSTATUS
QueueInitialize(
    _In_ WDFDEVICE Device
    );

EVT_WDF_IO_QUEUE_IO_READ           EvtIoRead;
EVT_WDF_IO_QUEUE_IO_WRITE          EvtIoWrite;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL EvtIoDeviceControl;

/* =========================================================================
 *  Function Prototypes — serial.c
 * ========================================================================= */

VOID SerialSetBaudRate     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetBaudRate     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetLineControl  (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetLineControl  (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetHandflow     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetHandflow     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetDtr          (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialClrDtr          (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetRts          (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialClrRts          (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetDtrRts       (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetModemStatus  (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetBreakOn      (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetBreakOff     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetTimeouts     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetTimeouts     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetChars        (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetChars        (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetQueueSize    (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialSetWaitMask     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetWaitMask     (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialWaitOnMask      (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialCompleteWaitOnMask (_In_ PDEVICE_CONTEXT DevCtx, _In_ ULONG Events);
VOID SerialPurge           (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetCommStatus   (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetProperties   (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialGetStats        (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialClearStats      (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialConfigSize      (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);
VOID SerialLsrMstInsert    (_In_ PDEVICE_CONTEXT DevCtx, _In_ WDFREQUEST Request);

/* =========================================================================
 *  Function Prototypes — usbtransfer.c
 * ========================================================================= */

EVT_WDF_USB_READER_COMPLETION_ROUTINE   EvtUsbBulkInReadComplete;
EVT_WDF_USB_READERS_FAILED              EvtUsbBulkInReadersFailed;
EVT_WDF_USB_READER_COMPLETION_ROUTINE   EvtUsbInterruptReadComplete;
EVT_WDF_USB_READERS_FAILED              EvtUsbInterruptReadersFailed;

EVT_WDF_REQUEST_COMPLETION_ROUTINE      EvtWriteRequestComplete;
EVT_WDF_WORKITEM                        EvtZlpWorkItem;

/* =========================================================================
 *  Function Prototypes — usbcontrol.c
 * ========================================================================= */

NTSTATUS
UsbControlSetLineCoding(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
UsbControlGetLineCoding(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
UsbControlSetControlLineState(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ USHORT          Value
    );

NTSTATUS
UsbControlSendBreak(
    _In_ PDEVICE_CONTEXT DevCtx,
    _In_ USHORT          Duration
    );

NTSTATUS
UsbControlSetDtr(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
UsbControlClrDtr(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
UsbControlSetRts(
    _In_ PDEVICE_CONTEXT DevCtx
    );

NTSTATUS
UsbControlClrRts(
    _In_ PDEVICE_CONTEXT DevCtx
    );

/* =========================================================================
 *  Function Prototypes — power.c
 * ========================================================================= */

NTSTATUS
PowerConfigureIdleSettings(
    _In_ PDEVICE_CONTEXT DevCtx
    );

/* =========================================================================
 *  Function Prototypes — wmi.c
 * ========================================================================= */

NTSTATUS
WmiRegistration(
    _In_ WDFDEVICE Device
    );

/* =========================================================================
 *  Ring Buffer Functions — usbtransfer.c
 * ========================================================================= */

NTSTATUS
RingBufferInit(
    _Inout_ PRING_BUFFER Ring,
    _In_    ULONG        Size
    );

VOID
RingBufferFree(
    _Inout_ PRING_BUFFER Ring
    );

ULONG
RingBufferWrite(
    _Inout_ PRING_BUFFER    Ring,
    _In_reads_(Length) PUCHAR Data,
    _In_    ULONG           Length
    );

ULONG
RingBufferRead(
    _Inout_ PRING_BUFFER    Ring,
    _Out_writes_(Length) PUCHAR Data,
    _In_    ULONG           Length
    );

ULONG
RingBufferGetDataLength(
    _In_ PRING_BUFFER Ring
    );

VOID
RingBufferPurge(
    _Inout_ PRING_BUFFER Ring
    );
