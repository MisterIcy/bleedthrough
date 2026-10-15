<?php

declare(strict_types=1);

namespace App\Http\Controller;

use App\Domain\Order;
use App\Domain\OrderRepository;
use App\Http\{Request, Response};
use InvalidArgumentException;

#[Route('/orders')]
final class OrderController
{
    private const PAGE_SIZE = 25;

    public function __construct(
        private readonly OrderRepository $orders,
        private ?string $locale = null,
    ) {
    }

    #[Route('/', methods: ['GET'])]
    public function index(Request $request): Response
    {
        $page = max(1, (int) ($request->query['page'] ?? 1));
        $items = $this->orders->page($page, self::PAGE_SIZE);

        return Response::json([
            'page' => $page,
            'items' => array_map(fn(Order $o): array => $o->toArray(), $items),
        ]);
    }

    #[Route('/{id}', methods: ['GET'])]
    public function show(Request $request, int $id): Response
    {
        $order = $this->orders->find($id) ?? throw new InvalidArgumentException("No order $id");

        return match (true) {
            $order->isCancelled() => Response::gone(),
            $order->isPaid() => Response::json($order->toArray()),
            default => Response::json($order->toArray() + ['payable' => true]),
        };
    }

    public function create(Request $request): Response
    {
        try {
            $order = Order::fromInput($request->body);
            $this->orders->save($order);
        } catch (InvalidArgumentException | \DomainException $e) {
            return Response::json(['error' => $e->getMessage()], status: 422);
        } finally {
            $this->locale ??= 'en';
        }

        return Response::json($order->toArray(), status: 201);
    }
}
