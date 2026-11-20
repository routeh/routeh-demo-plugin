// SPDX-License-Identifier: GPL-2.0-only
/*
 * The routeh demo plugin's device kind, "demo": a device that drops what
 * is sent through it and counts it, as drivers/net/dummy.c of Linux 6.12
 * does, with less. Made and removed over rtnetlink (ip link add x type
 * demo); each says so in the kernel's log, as loading and unloading do.
 */
#include <linux/etherdevice.h>
#include <linux/module.h>
#include <linux/netdevice.h>
#include <net/rtnetlink.h>

#define DRV_NAME "demo"

static netdev_tx_t demo_xmit(struct sk_buff *skb, struct net_device *dev)
{
	dev_lstats_add(dev, skb->len);
	dev_kfree_skb(skb);
	return NETDEV_TX_OK;
}

static int demo_dev_init(struct net_device *dev)
{
	netdev_info(dev, "made\n");
	return 0;
}

static void demo_dev_uninit(struct net_device *dev)
{
	netdev_info(dev, "removed\n");
}

static void demo_get_stats64(struct net_device *dev,
			     struct rtnl_link_stats64 *stats)
{
	dev_lstats_read(dev, &stats->tx_packets, &stats->tx_bytes);
}

static const struct net_device_ops demo_netdev_ops = {
	.ndo_init = demo_dev_init,
	.ndo_uninit = demo_dev_uninit,
	.ndo_start_xmit = demo_xmit,
	.ndo_validate_addr = eth_validate_addr,
	.ndo_set_mac_address = eth_mac_addr,
	.ndo_get_stats64 = demo_get_stats64,
};

static void demo_setup(struct net_device *dev)
{
	ether_setup(dev);
	dev->netdev_ops = &demo_netdev_ops;
	dev->needs_free_netdev = true;
	/* Its per-processor counters, allocated by the core (Linux 6.7). */
	dev->pcpu_stat_type = NETDEV_PCPU_STAT_LSTATS;
	dev->flags |= IFF_NOARP;
	dev->flags &= ~IFF_MULTICAST;
	dev->priv_flags |= IFF_LIVE_ADDR_CHANGE | IFF_NO_QUEUE;
	eth_hw_addr_random(dev);
	/* Any MTU: netifd sets the tunnel's. */
	dev->min_mtu = 0;
	dev->max_mtu = 0;
}

static struct rtnl_link_ops demo_link_ops __read_mostly = {
	.kind = DRV_NAME,
	.setup = demo_setup,
};

static int __init demo_init(void)
{
	int err = rtnl_link_register(&demo_link_ops);

	if (!err)
		pr_info(DRV_NAME ": loaded, devices of kind demo can be made\n");
	return err;
}

static void __exit demo_exit(void)
{
	rtnl_link_unregister(&demo_link_ops);
	pr_info(DRV_NAME ": unloaded\n");
}

module_init(demo_init);
module_exit(demo_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("The routeh demo plugin's device kind");
MODULE_ALIAS_RTNL_LINK(DRV_NAME);
