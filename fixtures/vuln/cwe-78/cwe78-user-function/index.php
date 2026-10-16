<?php

function handle(string $value): void
{
    system('ls -l ' . $value);
}

handle($_GET['x']);
