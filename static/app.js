class SyslogDashboard {
    constructor() {
        this.ws = null;
        this.logs = [];
        this.currentPage = 1;
        this.pageSize = 50;
        this.isPaused = false;
        this.autoRefreshEnabled = false;
        this.autoRefreshInterval = 10; // seconds
        this.autoRefreshTimer = null;
        this.filters = {
            search: '',
            facility: '',
            severity: ''
        };
        this.stats = {
            total_messages: 0,
            messages_per_facility: {},
            messages_per_severity: {},
            recent_sources: []
        };
        
        this.init();
    }

    init() {
        this.setupEventListeners();
        // Initialize connection status as disconnected first
        this.updateConnectionStatus(false);
        this.connectWebSocket();
        this.loadInitialData();
        this.startStatsUpdate();
    }

    setupEventListeners() {
        // Search input
        const searchInput = document.getElementById('searchInput');
        searchInput.addEventListener('input', this.debounce((e) => {
            this.filters.search = e.target.value;
            this.applyFilters();
        }, 300));

        // Filter selects
        document.getElementById('facilityFilter').addEventListener('change', (e) => {
            this.filters.facility = e.target.value;
            this.applyFilters();
        });

        document.getElementById('severityFilter').addEventListener('change', (e) => {
            this.filters.severity = e.target.value;
            this.applyFilters();
        });

        // Action buttons
        document.getElementById('refreshBtn').addEventListener('click', () => this.refreshLogs());
        document.getElementById('autoRefreshBtn').addEventListener('click', () => this.toggleAutoRefresh());
        document.getElementById('autoRefreshInterval').addEventListener('change', (e) => this.setAutoRefreshInterval(e.target.value));
        document.getElementById('pauseBtn').addEventListener('click', () => this.togglePause());
        document.getElementById('clearBtn').addEventListener('click', () => this.clearLogs());
        document.getElementById('exportBtn').addEventListener('click', () => this.exportLogs());

        // Pagination
        document.getElementById('prevBtn').addEventListener('click', () => this.previousPage());
        document.getElementById('nextBtn').addEventListener('click', () => this.nextPage());

        // Modal
        const modal = document.getElementById('logModal');
        const closeBtn = modal.querySelector('.close');
        closeBtn.addEventListener('click', () => this.closeModal());
        window.addEventListener('click', (e) => {
            if (e.target === modal) {
                this.closeModal();
            }
        });
        
        // Confirm Modal
        const confirmModal = document.getElementById('confirmModal');
        window.addEventListener('click', (e) => {
            if (e.target === confirmModal) {
                // Close modal when clicking outside
                if (this.confirmResolve) {
                    this.confirmResolve(false);
                    this.confirmResolve = null;
                }
                confirmModal.style.display = 'none';
            }
        });
    }

    connectWebSocket() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/api/ws`;
        
        this.ws = new WebSocket(wsUrl);
        
        this.ws.onopen = () => {
            console.log('WebSocket connected');
            this.updateConnectionStatus(true);
        };
        
        this.ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            
            // Handle connection confirmation message
            if (data.type === 'connected') {
                this.updateConnectionStatus(true);
                return;
            }
            
            // Handle syslog messages
            if (!this.isPaused) {
                this.addNewMessage(data);
            }
        };
        
        this.ws.onclose = (event) => {
            console.log('WebSocket disconnected');
            this.updateConnectionStatus(false);
            // Attempt to reconnect after 3 seconds
            setTimeout(() => this.connectWebSocket(), 3000);
        };
        
        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.updateConnectionStatus(false);
        };
    }

    updateConnectionStatus(connected) {
        const statusDot = document.getElementById('connectionStatus');
        const statusText = document.getElementById('connectionText');
        
        if (statusDot && statusText) {
            if (connected) {
                statusDot.classList.add('connected');
                statusText.textContent = '已连接';
            } else {
                statusDot.classList.remove('connected');
                statusText.textContent = '连接断开';
            }
        }
    }

    async loadInitialData() {
        try {
            const response = await fetch('/api/logs?limit=100');
            const logs = await response.json();
            this.logs = logs;
            this.renderLogs();
            this.updatePagination();
        } catch (error) {
            console.error('Failed to load initial data:', error);
        }
    }

    async loadStats() {
        try {
            const response = await fetch('/api/stats');
            this.stats = await response.json();
            this.updateStatsDisplay();
        } catch (error) {
            console.error('Failed to load stats:', error);
        }
    }

    startStatsUpdate() {
        this.loadStats();
        setInterval(() => this.loadStats(), 5000); // Update every 5 seconds
    }

    updateStatsDisplay() {
        document.getElementById('totalMessages').textContent = this.stats.total_messages.toLocaleString();
        
        // Count error messages (severity 0-3)
        const errorCount = Object.entries(this.stats.messages_per_severity)
            .filter(([severity]) => parseInt(severity) <= 3)
            .reduce((sum, [, count]) => sum + count, 0);
        document.getElementById('errorCount').textContent = errorCount.toLocaleString();
        
        // Count info messages (severity 6)
        const infoCount = this.stats.messages_per_severity[6] || 0;
        document.getElementById('infoCount').textContent = infoCount.toLocaleString();
        
        // Active sources count
        document.getElementById('sourceCount').textContent = this.stats.recent_sources.length;
        
        // Update facility filter options
        this.updateFacilityFilter();
    }

    updateFacilityFilter() {
        const facilityFilter = document.getElementById('facilityFilter');
        const currentValue = facilityFilter.value;
        
        // Clear existing options except the first one
        while (facilityFilter.children.length > 1) {
            facilityFilter.removeChild(facilityFilter.lastChild);
        }
        
        // Add facility options
        const facilityNames = {
            0: 'Kernel', 1: 'User', 2: 'Mail', 3: 'Daemon', 4: 'Security',
            5: 'Syslog', 6: 'Line Printer', 7: 'Network News', 8: 'UUCP',
            9: 'Clock', 10: 'Security', 11: 'FTP', 12: 'NTP', 13: 'Log Audit',
            14: 'Log Alert', 15: 'Clock', 16: 'Local0', 17: 'Local1',
            18: 'Local2', 19: 'Local3', 20: 'Local4', 21: 'Local5',
            22: 'Local6', 23: 'Local7'
        };
        
        Object.entries(this.stats.messages_per_facility).forEach(([facility, count]) => {
            const option = document.createElement('option');
            option.value = facility;
            option.textContent = `${facilityNames[facility] || `Facility ${facility}`} (${count})`;
            facilityFilter.appendChild(option);
        });
        
        facilityFilter.value = currentValue;
    }

    addNewMessage(message) {
        this.logs.unshift(message);
        
        // Limit the number of logs in memory
        if (this.logs.length > 1000) {
            this.logs = this.logs.slice(0, 1000);
        }
        
        // If we're on the first page and no filters are applied, update the display
        if (this.currentPage === 1 && this.isFilterEmpty()) {
            this.renderLogs();
        }
    }

    isFilterEmpty() {
        return !this.filters.search && !this.filters.facility && !this.filters.severity;
    }

    applyFilters() {
        this.currentPage = 1;
        this.renderLogs();
        this.updatePagination();
    }

    getFilteredLogs() {
        return this.logs.filter(log => {
            // Search filter
            if (this.filters.search) {
                const searchTerm = this.filters.search.toLowerCase();
                const searchableText = [
                    log.message,
                    log.hostname || '',
                    log.app_name || '',
                    log.source_ip
                ].join(' ').toLowerCase();
                
                if (!searchableText.includes(searchTerm)) {
                    return false;
                }
            }
            
            // Facility filter
            if (this.filters.facility && log.facility.toString() !== this.filters.facility) {
                return false;
            }
            
            // Severity filter
            if (this.filters.severity && log.severity.toString() !== this.filters.severity) {
                return false;
            }
            
            return true;
        });
    }

    renderLogs() {
        const tbody = document.getElementById('logsTableBody');
        const filteredLogs = this.getFilteredLogs();
        
        const startIndex = (this.currentPage - 1) * this.pageSize;
        const endIndex = startIndex + this.pageSize;
        const pageData = filteredLogs.slice(startIndex, endIndex);
        
        tbody.innerHTML = '';
        
        pageData.forEach(log => {
            const row = this.createLogRow(log);
            tbody.appendChild(row);
        });
    }

    createLogRow(log) {
        const row = document.createElement('tr');
        row.className = 'new-message';
        
        const severityNames = {
            0: 'EMERG', 1: 'ALERT', 2: 'CRIT', 3: 'ERR',
            4: 'WARN', 5: 'NOTICE', 6: 'INFO', 7: 'DEBUG'
        };
        
        const timestamp = new Date(log.timestamp).toLocaleString('zh-CN');
        const severityName = severityNames[log.severity] || log.severity;
        
        row.innerHTML = `
            <td>${timestamp}</td>
            <td><span class="severity-badge severity-${log.severity}">${severityName}</span></td>
            <td><span class="facility-badge">${log.facility}</span></td>
            <td>${log.hostname || '-'}</td>
            <td>${log.app_name || '-'}</td>
            <td class="message-cell" title="${this.escapeHtml(log.message)}">${this.escapeHtml(log.message)}</td>
            <td>${log.source_ip}</td>
            <td><button class="view-btn" onclick="dashboard.showLogDetail('${log.id}')">查看</button></td>
        `;
        
        return row;
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    showLogDetail(logId) {
        const log = this.logs.find(l => l.id === logId);
        if (!log) return;
        
        const modalBody = document.getElementById('logModalBody');
        modalBody.innerHTML = `
            <div class="log-detail">
                <div class="log-field">
                    <label>ID</label>
                    <div class="value">${log.id}</div>
                </div>
                <div class="log-field">
                    <label>时间戳</label>
                    <div class="value">${new Date(log.timestamp).toLocaleString('zh-CN')}</div>
                </div>
                <div class="log-field">
                    <label>设施 (Facility)</label>
                    <div class="value">${log.facility}</div>
                </div>
                <div class="log-field">
                    <label>严重性 (Severity)</label>
                    <div class="value">${log.severity}</div>
                </div>
                <div class="log-field">
                    <label>主机名</label>
                    <div class="value">${log.hostname || '未指定'}</div>
                </div>
                <div class="log-field">
                    <label>应用名称</label>
                    <div class="value">${log.app_name || '未指定'}</div>
                </div>
                <div class="log-field">
                    <label>进程ID</label>
                    <div class="value">${log.proc_id || '未指定'}</div>
                </div>
                <div class="log-field">
                    <label>消息ID</label>
                    <div class="value">${log.msg_id || '未指定'}</div>
                </div>
                <div class="log-field">
                    <label>源IP地址</label>
                    <div class="value">${log.source_ip}</div>
                </div>
                <div class="log-field">
                    <label>消息内容</label>
                    <div class="value">${this.escapeHtml(log.message)}</div>
                </div>
                <div class="log-field">
                    <label>原始消息</label>
                    <div class="value">${this.escapeHtml(log.raw_message)}</div>
                </div>
            </div>
        `;
        
        document.getElementById('logModal').style.display = 'block';
    }

    showConfirmDialog(title, message) {
        console.log('=== showConfirmDialog 被调用 ===');
        console.log('title:', title, 'message:', message);
        console.log('document.readyState:', document.readyState);
        
        return new Promise((resolve) => {
            // 确保DOM完全加载后再查找元素
            const findElements = () => {
                const modal = document.getElementById('confirmModal');
                const titleElement = document.getElementById('confirmTitle');
                const messageElement = document.getElementById('confirmMessage');
                const confirmOk = document.getElementById('confirmOk');
                const confirmCancel = document.getElementById('confirmCancel');
                
                console.log('查找结果:', {
                    modal: modal,
                    titleElement: titleElement,
                    messageElement: messageElement,
                    confirmOk: confirmOk,
                    confirmCancel: confirmCancel
                });
                
                // 如果所有元素都存在，直接使用
                if (modal && titleElement && messageElement && confirmOk && confirmCancel) {
                    console.log('所有元素都找到了，使用原生对话框');
                    this.setupConfirmDialog(modal, titleElement, messageElement, confirmOk, confirmCancel, title, message)
                        .then(resolve);
                } else {
                    console.log('元素未找到，使用临时对话框');
                    this.createTemporaryConfirmDialog(title, message, resolve);
                }
            };
            
            // 如果DOM已经加载完成，直接查找
            if (document.readyState === 'complete' || document.readyState === 'interactive') {
                findElements();
            } else {
                // 否则等待DOM加载完成
                document.addEventListener('DOMContentLoaded', findElements);
            }
        });
    }
    
    setupConfirmDialog(modal, titleElement, messageElement, confirmOk, confirmCancel, title, message) {
        return new Promise((resolve) => {
            // Store resolve function for external access
            this.confirmResolve = resolve;
            
            // Set dialog content
            titleElement.textContent = title;
            messageElement.textContent = message;
            
            // Remove any existing event listeners by cloning nodes
            const newConfirmOk = confirmOk.cloneNode(true);
            const newConfirmCancel = confirmCancel.cloneNode(true);
            confirmOk.parentNode.replaceChild(newConfirmOk, confirmOk);
            confirmCancel.parentNode.replaceChild(newConfirmCancel, confirmCancel);
            
            // Add event listeners
            newConfirmOk.addEventListener('click', () => {
                modal.style.display = 'none';
                if (this.confirmResolve) {
                    this.confirmResolve(true);
                    this.confirmResolve = null;
                }
            });
            
            newConfirmCancel.addEventListener('click', () => {
                modal.style.display = 'none';
                if (this.confirmResolve) {
                    this.confirmResolve(false);
                    this.confirmResolve = null;
                }
            });
            
            // Show modal
            modal.style.display = 'block';
        });
    }
    


    createTemporaryConfirmDialog(title, message, resolve) {
        // 创建模态框容器
        const modal = document.createElement('div');
        modal.id = 'tempConfirmModal';
        modal.className = 'modal';
        modal.style.display = 'block';
        
        // 创建模态框内容
        modal.innerHTML = `
            <div class="modal-content confirm-modal">
                <div class="modal-header">
                    <h2>${title}</h2>
                </div>
                <div class="modal-body">
                    <p>${message}</p>
                </div>
                <div class="modal-footer">
                    <button id="tempConfirmCancel" class="btn btn-secondary">
                        <i class="fas fa-times"></i> 取消
                    </button>
                    <button id="tempConfirmOk" class="btn btn-danger">
                        <i class="fas fa-check"></i> 确定
                    </button>
                </div>
            </div>
        `;
        
        // 添加到页面
        document.body.appendChild(modal);
        
        // 绑定事件
        const okBtn = modal.querySelector('#tempConfirmOk');
        const cancelBtn = modal.querySelector('#tempConfirmCancel');
        
        const cleanup = () => {
            document.body.removeChild(modal);
        };
        
        okBtn.addEventListener('click', () => {
            cleanup();
            resolve(true);
        });
        
        cancelBtn.addEventListener('click', () => {
            cleanup();
            resolve(false);
        });
        
        // 点击外部关闭
        modal.addEventListener('click', (e) => {
            if (e.target === modal) {
                cleanup();
                resolve(false);
            }
        });
    }

    showNotification(message, type = 'info') {
        const notification = document.createElement('div');
        notification.className = `notification notification-${type}`;
        notification.textContent = message;
        
        document.body.appendChild(notification);
        
        // Show notification
        setTimeout(() => {
            notification.classList.add('show');
        }, 100);
        
        // Hide notification after 3 seconds
        setTimeout(() => {
            notification.classList.remove('show');
            setTimeout(() => {
                document.body.removeChild(notification);
            }, 300);
        }, 3000);
    }

    closeModal() {
        document.getElementById('logModal').style.display = 'none';
    }

    togglePause() {
        this.isPaused = !this.isPaused;
        const pauseBtn = document.getElementById('pauseBtn');
        
        if (this.isPaused) {
            pauseBtn.innerHTML = '<i class="fas fa-play"></i> 继续';
            pauseBtn.classList.add('btn-primary');
        } else {
            pauseBtn.innerHTML = '<i class="fas fa-pause"></i> 暂停';
            pauseBtn.classList.remove('btn-primary');
        }
    }

    async clearLogs() {
        const confirmed = await this.showConfirmDialog(
            '确认清空日志',
            '确定要清空所有日志吗？此操作不可撤销。'
        );
        
        if (confirmed) {
            try {
                const response = await fetch('/api/logs', {
                    method: 'DELETE'
                });
                
                if (response.ok) {
                    // Clear frontend logs
                    this.logs = [];
                    this.currentPage = 1;
                    this.renderLogs();
                    this.updatePagination();
                    
                    // Reset stats
                    this.stats = {
                        total_messages: 0,
                        messages_per_facility: {},
                        messages_per_severity: {},
                        recent_sources: []
                    };
                    this.updateStatsDisplay();
                    
                    this.showNotification('所有日志已清空', 'success');
                    console.log('所有日志已清空');
                } else {
                    console.error('清空日志失败:', response.statusText);
                    this.showNotification('清空日志失败，请重试', 'error');
                }
            } catch (error) {
                console.error('清空日志时发生错误:', error);
                this.showNotification('清空日志时发生错误，请重试', 'error');
            }
        }
    }

    exportLogs() {
        const filteredLogs = this.getFilteredLogs();
        const csvContent = this.convertToCSV(filteredLogs);
        const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
        const link = document.createElement('a');
        
        if (link.download !== undefined) {
            const url = URL.createObjectURL(blob);
            link.setAttribute('href', url);
            link.setAttribute('download', `syslog_export_${new Date().toISOString().slice(0, 10)}.csv`);
            link.style.visibility = 'hidden';
            document.body.appendChild(link);
            link.click();
            document.body.removeChild(link);
        }
    }

    convertToCSV(logs) {
        const headers = ['时间戳', '设施', '严重性', '主机名', '应用名称', '进程ID', '消息ID', '源IP', '消息内容', '原始消息'];
        const csvRows = [headers.join(',')];
        
        logs.forEach(log => {
            const row = [
                new Date(log.timestamp).toISOString(),
                log.facility,
                log.severity,
                log.hostname || '',
                log.app_name || '',
                log.proc_id || '',
                log.msg_id || '',
                log.source_ip,
                `"${log.message.replace(/"/g, '""')}"`,
                `"${log.raw_message.replace(/"/g, '""')}"`
            ];
            csvRows.push(row.join(','));
        });
        
        return csvRows.join('\n');
    }

    previousPage() {
        if (this.currentPage > 1) {
            this.currentPage--;
            this.renderLogs();
            this.updatePagination();
        }
    }

    nextPage() {
        const filteredLogs = this.getFilteredLogs();
        const totalPages = Math.ceil(filteredLogs.length / this.pageSize);
        
        if (this.currentPage < totalPages) {
            this.currentPage++;
            this.renderLogs();
            this.updatePagination();
        }
    }

    updatePagination() {
        const filteredLogs = this.getFilteredLogs();
        const totalPages = Math.ceil(filteredLogs.length / this.pageSize);
        
        document.getElementById('prevBtn').disabled = this.currentPage <= 1;
        document.getElementById('nextBtn').disabled = this.currentPage >= totalPages;
        document.getElementById('pageInfo').textContent = `第 ${this.currentPage} 页，共 ${totalPages} 页 (${filteredLogs.length} 条记录)`;
    }

    async refreshLogs() {
        const refreshBtn = document.getElementById('refreshBtn');
        const icon = refreshBtn.querySelector('i');
        
        // Add spinning animation
        icon.classList.add('fa-spin');
        refreshBtn.disabled = true;
        
        try {
            // Reload logs and stats
            await this.loadInitialData();
            await this.loadStats();
            console.log('日志已刷新');
        } catch (error) {
            console.error('刷新失败:', error);
        } finally {
            // Remove spinning animation
            icon.classList.remove('fa-spin');
            refreshBtn.disabled = false;
        }
    }

    toggleAutoRefresh() {
        this.autoRefreshEnabled = !this.autoRefreshEnabled;
        const autoRefreshBtn = document.getElementById('autoRefreshBtn');
        const intervalSelect = document.getElementById('autoRefreshInterval');
        
        if (this.autoRefreshEnabled) {
            // Start auto refresh
            this.startAutoRefresh();
            autoRefreshBtn.classList.add('active');
            autoRefreshBtn.innerHTML = '<i class="fas fa-clock"></i> 停止自动刷新';
            intervalSelect.disabled = true;
        } else {
            // Stop auto refresh
            this.stopAutoRefresh();
            autoRefreshBtn.classList.remove('active');
            autoRefreshBtn.innerHTML = '<i class="fas fa-clock"></i> 自动刷新';
            intervalSelect.disabled = false;
        }
    }

    setAutoRefreshInterval(seconds) {
        this.autoRefreshInterval = parseInt(seconds);
        
        if (this.autoRefreshInterval === 0) {
            // Disable auto refresh if interval is 0
            if (this.autoRefreshEnabled) {
                this.toggleAutoRefresh();
            }
        } else if (this.autoRefreshEnabled) {
            // Restart auto refresh with new interval
            this.stopAutoRefresh();
            this.startAutoRefresh();
        }
    }

    startAutoRefresh() {
        if (this.autoRefreshInterval > 0) {
            this.autoRefreshTimer = setInterval(() => {
                if (!this.isPaused) {
                    this.refreshLogs();
                }
            }, this.autoRefreshInterval * 1000);
            console.log(`自动刷新已启动，间隔: ${this.autoRefreshInterval}秒`);
        }
    }

    stopAutoRefresh() {
        if (this.autoRefreshTimer) {
            clearInterval(this.autoRefreshTimer);
            this.autoRefreshTimer = null;
            console.log('自动刷新已停止');
        }
    }

    debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    }
}

// Initialize the dashboard when the page loads
let dashboard;
document.addEventListener('DOMContentLoaded', () => {
    dashboard = new SyslogDashboard();
});

// Handle page visibility changes to pause/resume when tab is not active
document.addEventListener('visibilitychange', () => {
    if (dashboard) {
        if (document.hidden) {
            // Page is hidden, pause auto refresh to save resources
            if (dashboard.autoRefreshEnabled) {
                dashboard.stopAutoRefresh();
            }
        } else {
            // Page is visible, resume auto refresh and load stats
            if (dashboard.autoRefreshEnabled) {
                dashboard.startAutoRefresh();
            }
            dashboard.loadStats();
        }
    }
});

// Clean up auto refresh timer when page is unloaded
window.addEventListener('beforeunload', () => {
    if (dashboard && dashboard.autoRefreshTimer) {
        dashboard.stopAutoRefresh();
    }
});