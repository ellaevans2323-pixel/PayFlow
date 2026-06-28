# Keeper Bot Operations Handbook

## Overview

Keeper bots are autonomous agents that invoke the `batch_charge()` contract function to process recurring billing charges for all active subscriptions. This runbook documents operational procedures, monitoring requirements, and troubleshooting strategies for production deployments.

## Keeper Bot Responsibilities

- **Periodic charge execution** - Call `batch_charge()` at scheduled intervals (e.g., every hour)
- **Paginated index processing** - Iterate through subscriber base using offset/limit parameters
- **Balance monitoring** - Track keeper account balance to ensure sufficient gas/tokens
- **Error recovery** - Implement retry logic and alerting for failed invocations

## Pagination Mechanics

### Understanding Subscriber Pages

The contract stores subscriptions in a paginated index. Each `batch_charge()` call processes a single page:

```
Total Subscriptions: 542
Page Size: 100 subscriptions/page

Page 0: Subscriptions 0-99
Page 1: Subscriptions 100-199
Page 2: Subscriptions 200-299
Page 3: Subscriptions 300-399
Page 4: Subscriptions 400-499
Page 5: Subscriptions 500-542 (partial page)
```

### Sequential Page Processing

```python
#!/usr/bin/env python3
"""Keeper bot batch charge loop"""

import time
import logging
from soroban_client import SorobanClient

logger = logging.getLogger(__name__)

class KeeperBot:
    def __init__(self, contract_id, keeper_keypair, rpc_url):
        self.client = SorobanClient(rpc_url)
        self.contract_id = contract_id
        self.keeper_keypair = keeper_keypair
        self.page_size = 100
        
    def process_all_pages(self):
        """Process all subscription pages sequentially"""
        page_offset = 0
        total_charged = 0
        
        while True:
            try:
                # Invoke batch_charge for current page
                result = self.client.invoke_contract(
                    self.contract_id,
                    "batch_charge",
                    {
                        "page_offset": page_offset,
                        "page_size": self.page_size,
                    },
                    signer=self.keeper_keypair,
                )
                
                charged_count = result.charged
                total_charged += charged_count
                logger.info(f"Page {page_offset}: Charged {charged_count} subscriptions")
                
                # If page returned fewer items than requested, we've reached the end
                if charged_count < self.page_size:
                    logger.info(f"Completed cycle: {total_charged} total subscriptions charged")
                    break
                
                page_offset += self.page_size
                
            except Exception as e:
                logger.error(f"Failed to process page {page_offset}: {e}")
                self.alert_operator("batch_charge failure", str(e))
                break
        
        return total_charged
    
    def run_keeper_loop(self, interval_seconds=3600):
        """Main keeper loop: run batch_charge every interval_seconds"""
        while True:
            try:
                logger.info("Starting batch_charge cycle")
                self.check_keeper_balance()
                charged = self.process_all_pages()
                logger.info(f"Cycle complete. Next run in {interval_seconds}s")
                
            except Exception as e:
                logger.error(f"Keeper loop error: {e}")
                self.alert_operator("keeper_loop_error", str(e))
            
            time.sleep(interval_seconds)
```

### Page Size Considerations

```
MAX_PAGE_SIZE: 100 subscriptions per batch_charge call

Rationale:
- Prevents single transaction from exceeding Soroban gas limits
- Allows keeper to process ~500-1000 subscriptions per hour
- Enables horizontal scaling (multiple keepers process different page ranges)
```

## Monitoring & Alerting

### Critical Metrics

| Metric | Threshold | Action |
|--------|-----------|--------|
| **Keeper Account Balance** | < 10 XLM | CRITICAL - Refund keeper |
| **Batch Charge Latency** | > 30 seconds | WARNING - Check network |
| **Failed Charges** | > 5% of page | WARNING - Review error logs |
| **Page Processing Time** | > 60 seconds | WARNING - Possible network congestion |
| **Keeper Availability** | 100% uptime | CRITICAL - Ensure HA setup |

### Health Check Implementation

```python
def health_check(self):
    """Diagnostic health check for keeper status"""
    health = {
        "keeper_balance": self.get_balance(),
        "last_charge_time": self.get_last_charge_time(),
        "failed_charges": self.get_failed_charge_count(),
        "time_since_last_cycle": time.time() - self.last_cycle_time,
    }
    
    alerts = []
    
    if health["keeper_balance"] < 10e7:  # 10 XLM in stroops
        alerts.append("CRITICAL: Keeper balance low")
    
    if health["time_since_last_cycle"] > 7200:  # 2 hours
        alerts.append("CRITICAL: No successful charge cycle in 2 hours")
    
    if health["failed_charges"] > 0:
        alerts.append(f"WARNING: {health['failed_charges']} failed charges")
    
    return health, alerts
```

### Example Monitoring Stack

```yaml
# prometheus-keeper.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'keeper_metrics'
    static_configs:
      - targets: ['localhost:8000']

alerting:
  alertmanagers:
    - static_configs:
        - targets: ['alertmanager:9093']

rule_files:
  - 'keeper_alerts.yml'
```

```yaml
# keeper_alerts.yml
groups:
  - name: keeper_alerts
    rules:
      - alert: KeeperBalanceLow
        expr: keeper_balance_stroops < 1000000000
        for: 5m
        annotations:
          summary: "Keeper balance critically low"
      
      - alert: BatchChargeFailure
        expr: rate(batch_charge_failures[5m]) > 0
        for: 5m
        annotations:
          summary: "Batch charge operation failed"
      
      - alert: KeeperNotResponding
        expr: time() - keeper_last_ping > 3600
        for: 5m
        annotations:
          summary: "Keeper bot not responding"
```

## Failure Modes & Troubleshooting

### 1. Low Keeper Balance

**Symptom:** Batch charge fails with "insufficient balance" error

**Root Cause:** Keeper account has insufficient XLM for transaction fees

**Resolution:**
```bash
# Check current balance
stellar account info keeper_account

# Fund keeper account
stellar payment --from funding_account \
  --to keeper_account \
  --amount 100 --asset native

# Verify funding
stellar account info keeper_account
```

### 2. Transaction Parsing Failures

**Symptom:** "InvalidArgument" errors during migration or contract upgrade

**Root Cause:** Contract signature changed during deployment; keeper still using old ABI

**Resolution:**
```bash
# Update keeper bot with new contract ABI
# Redeploy contract with new signature
# Restart keeper with new binary

# Verify contract interface
stellar contract read --id CONTRACT_ID
```

### 3. Pagination Offset Overflow

**Symptom:** Keeper hangs or returns empty pages unexpectedly

**Root Cause:** Page offset exceeds total subscription count; no early termination

**Resolution:**
```python
# Keeper should check for empty pages and exit loop
if charged_count == 0 and page_offset > 0:
    logger.info("End of subscription list reached")
    break

# Or implement subscription count check
total_subs = client.invoke_contract(
    contract_id, "get_subscription_count", {}
)
max_pages = (total_subs + page_size - 1) // page_size
```

### 4. Network Congestion

**Symptom:** Batch charge latency increases dramatically

**Root Cause:** High load on RPC node or Stellar network congestion

**Monitoring & Response:**
```python
# Track latency percentiles
latency_p95 = get_latency_percentile(95)

if latency_p95 > 30000:  # 30 seconds
    logger.warning("High network latency detected")
    # Increase backoff
    backoff_multiplier = 2.0
    # Alert ops team
    send_alert("Network_Degradation", {"p95_latency": latency_p95})
```

## Keeper Deployment Patterns

### Single Keeper (Non-HA)

```bash
# Simple cron-based keeper
0 * * * * /opt/keeper/run_batch_charge.sh >> /var/log/keeper.log 2>&1
```

**Risks:**
- Single point of failure
- Missed billing cycles if keeper offline
- No redundancy

### Multiple Keeper (HA) with Leader Election

```python
# leader_election.py
import redis

class KeeperCluster:
    def __init__(self):
        self.redis = redis.Redis(host='redis-leader', port=6379)
        self.keeper_id = os.getenv('KEEPER_ID')
    
    def acquire_leadership(self):
        """Attempt to become the active keeper"""
        acquired = self.redis.set(
            'keeper:leader',
            self.keeper_id,
            ex=300,  # 5-minute lease
            nx=True  # Only if key doesn't exist
        )
        return acquired
    
    def maintain_leadership(self):
        """Renew leadership lease"""
        while True:
            if self.acquire_leadership():
                self.process_batch_charge()
            time.sleep(60)
```

### Keeper Infrastructure as Code

```hcl
# terraform/keeper.tf
resource "kubernetes_deployment" "keeper" {
  metadata {
    name      = "payflow-keeper"
    namespace = "production"
  }

  spec {
    replicas = 3
    
    template {
      spec {
        container {
          name  = "keeper"
          image = "payflow/keeper:latest"
          env {
            name  = "KEEPER_ID"
            value = "keeper-${pod.metadata.name}"
          }
          resources {
            requests = {
              cpu    = "100m"
              memory = "128Mi"
            }
          }
        }
      }
    }
  }
}
```

## Operational Checklist

### Pre-Launch Verification

- [ ] Keeper account funded with sufficient balance (minimum 100 XLM)
- [ ] Contract ABI matches keeper bot expectations
- [ ] Pagination loop tested with production subscriber volume
- [ ] Monitoring and alerting rules deployed
- [ ] Backup/redundancy keeper configured
- [ ] RPC endpoints verified healthy
- [ ] Network connectivity from keeper host to Soroban RPC confirmed

### Post-Launch Monitoring

- [ ] Keeper metrics flowing into monitoring stack
- [ ] Alert thresholds reviewed and appropriate
- [ ] Batch charge cycle latency < 60 seconds
- [ ] No recurring charge failures
- [ ] Keeper availability > 99.9%
- [ ] Regular balance top-ups scheduled

### Incident Response

1. **Keeper bot unresponsive**
   - Check process status: `ps aux | grep keeper`
   - Review logs: `tail -f /var/log/keeper.log`
   - Restart if hung: `systemctl restart keeper`

2. **Transaction failures**
   - Check keeper balance
   - Verify contract is not frozen
   - Inspect error logs for transaction specifics

3. **Billing cycle missed**
   - Check keeper uptime during missed window
   - Manually invoke missed batch_charge pages
   - Add padding time to next scheduled run
